//! GPU pipeline cache for @flow shaders
//!
//! Compiles FlowGraph → WGSL → wgpu pipeline on first use, caches by flow name.
//! Handles both fragment (render) and compute (simulation) pipelines.
//! Uniform buffers are updated per-frame with runtime data (time, pointer, etc.).
//!
//! ## Memory optimizations
//!
//! - **Bind group caching**: Bind groups are created once per flow (or once per scene
//!   texture change) and reused across frames. Only the uniform buffer is written each
//!   frame via `queue.write_buffer()`.
//! - **Right-sized storage buffers**: Storage buffers start at 1K elements (not 64K)
//!   and are only allocated when the flow actually uses buffer I/O.
//! - **LRU pipeline eviction**: The pipeline cache holds at most `MAX_CACHED_PIPELINES`
//!   entries. When full, the least-recently-used pipeline is evicted.

use std::collections::HashMap;
use std::sync::Arc;

use blinc_core::{FlowGraph, FlowInputSource, FlowTarget, FlowType};

use crate::flow_codegen::{flow_needs_scene_texture, flow_to_wgsl};

/// Maximum number of compiled flow pipelines kept in cache.
/// When exceeded, the least-recently-used pipeline is evicted.
const MAX_CACHED_PIPELINES: usize = 24;

/// Default storage buffer element count (1K instead of 64K).
const DEFAULT_BUFFER_ELEMENTS: u64 = 1024;

// ==========================================================================
// Uniform Data
// ==========================================================================

/// Runtime data written to the flow uniform buffer each frame.
///
/// Layout matches the `FlowUniforms` struct emitted by `flow_codegen.rs`.
/// Fields are tightly packed as vec4-aligned f32 arrays for GPU upload.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FlowUniformData {
    /// Viewport size in pixels
    pub viewport_size: [f32; 2],
    /// Elapsed time in seconds
    pub time: f32,
    /// Frame index (monotonic counter)
    pub frame_index: f32,
    /// Element bounds: [x, y, width, height] in pixels
    pub element_bounds: [f32; 4],
    /// Pointer position (from pointer query system)
    pub pointer: [f32; 2],
    /// Corner radius in pixels (for rounded-rect clipping)
    pub corner_radius: f32,
    /// Press intensity (0.0 released, 1.0 pressed). Driven by the
    /// renderer directly from cursor + button-pressed state, so any
    /// `@flow`-using element gets a press envelope without needing to
    /// register `pointer-space:` in the stylesheet (which only fires
    /// for id-keyed rules).
    pub pressure: f32,
}

impl Default for FlowUniformData {
    fn default() -> Self {
        Self {
            viewport_size: [0.0; 2],
            time: 0.0,
            frame_index: 0.0,
            element_bounds: [0.0; 4],
            pointer: [0.0; 2],
            corner_radius: 0.0,
            pressure: 0.0,
        }
    }
}

// ==========================================================================
// Cached Pipeline
// ==========================================================================

/// A compiled and cached flow pipeline ready for rendering
struct CachedFlowPipeline {
    /// The compiled WGSL source (kept for debugging)
    #[allow(dead_code)]
    wgsl_source: String,
    /// Render pipeline (for fragment flows)
    render_pipeline: Option<wgpu::RenderPipeline>,
    /// Compute pipeline (for compute flows)
    compute_pipeline: Option<wgpu::ComputePipeline>,
    /// Bind group layout (shared between render and compute)
    bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform buffer (updated per-frame)
    uniform_buffer: wgpu::Buffer,
    /// Storage buffers for compute I/O, keyed by name
    storage_buffers: HashMap<String, wgpu::Buffer>,
    /// The flow target type
    target: FlowTarget,
    /// Size of the uniform struct in bytes (base + dynamic CSS/env fields)
    uniform_size: u64,
    /// Whether this flow uses sample_scene() and needs a scene texture binding
    needs_scene: bool,
    /// Binding index for the scene texture (if needs_scene)
    scene_texture_binding: u32,
    /// Cached bind group (reused across frames). Invalidated when scene texture changes.
    cached_bind_group: Option<wgpu::BindGroup>,
    /// The scene texture view pointer used when the cached bind group was built.
    /// If the scene texture changes (resize), the bind group must be rebuilt.
    cached_scene_id: u64,
}

// ==========================================================================
// Pipeline Cache
// ==========================================================================

/// Cache of compiled @flow GPU pipelines.
///
/// Created once per renderer and persists across frames.
/// Pipelines are compiled lazily on first use.
pub struct FlowPipelineCache {
    device: Arc<wgpu::Device>,
    texture_format: wgpu::TextureFormat,
    pipelines: HashMap<String, CachedFlowPipeline>,
    /// LRU order: most-recently-used at the back
    lru_order: Vec<String>,
    /// Sampler for scene texture sampling (linear filtering)
    scene_sampler: wgpu::Sampler,
    /// 1x1 transparent dummy texture for flows that don't use scene sampling
    dummy_scene_view: wgpu::TextureView,
    /// Monotonic ID for scene texture changes (incremented on each new scene texture)
    scene_generation: u64,
}

impl FlowPipelineCache {
    /// Create a new empty pipeline cache
    pub fn new(device: Arc<wgpu::Device>, texture_format: wgpu::TextureFormat) -> Self {
        let scene_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Flow Scene Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // 1x1 transparent dummy texture for flows that don't use scene
        let dummy_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Flow Dummy Scene Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let dummy_scene_view = dummy_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            device,
            texture_format,
            pipelines: HashMap::new(),
            lru_order: Vec::new(),
            scene_sampler,
            dummy_scene_view,
            scene_generation: 0,
        }
    }

    /// Check if a flow pipeline is already compiled
    pub fn contains(&self, flow_name: &str) -> bool {
        self.pipelines.contains_key(flow_name)
    }

    /// Bump a flow to the most-recently-used position
    fn touch_lru(&mut self, name: &str) {
        if let Some(pos) = self.lru_order.iter().position(|n| n == name) {
            self.lru_order.remove(pos);
        }
        self.lru_order.push(name.to_string());
    }

    /// Evict the least-recently-used pipeline if over capacity
    fn evict_if_needed(&mut self) {
        while self.pipelines.len() >= MAX_CACHED_PIPELINES && !self.lru_order.is_empty() {
            let oldest = self.lru_order.remove(0);
            if self.pipelines.remove(&oldest).is_some() {
                tracing::debug!("Flow pipeline cache: evicted '{}'", oldest);
            }
        }
    }

    /// Notify the cache that the scene texture has changed (e.g., viewport resize).
    /// This invalidates all cached bind groups for flows that use scene sampling.
    pub fn invalidate_scene_bind_groups(&mut self) {
        self.scene_generation += 1;
    }

    /// Compile a FlowGraph into a GPU pipeline and cache it.
    ///
    /// Returns Ok(()) on success, or an error string on compilation failure.
    /// If the pipeline already exists, this is a no-op.
    pub fn compile(&mut self, graph: &FlowGraph) -> Result<(), String> {
        if self.pipelines.contains_key(&graph.name) {
            self.touch_lru(&graph.name);
            return Ok(());
        }

        // Evict oldest pipeline if at capacity
        self.evict_if_needed();

        // Generate WGSL
        let wgsl = flow_to_wgsl(graph).map_err(|e| format!("codegen error: {}", e))?;

        // Dump generated WGSL for debugging
        if let Ok(path) = std::env::var("BLINC_DUMP_WGSL") {
            let file = format!("{}/flow_{}.wgsl", path, graph.name);
            let _ = std::fs::write(&file, &wgsl);
            tracing::info!("Dumped @flow '{}' WGSL to {}", graph.name, file);
        }

        // Create shader module
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("Flow Shader: {}", graph.name)),
                source: wgpu::ShaderSource::Wgsl(wgsl.clone().into()),
            });

        // Count dynamic uniform fields (CSS properties + env vars)
        let dynamic_field_count = graph
            .inputs
            .iter()
            .filter(|i| {
                matches!(
                    i.source,
                    FlowInputSource::CssProperty(_) | FlowInputSource::EnvVar(_)
                )
            })
            .count();

        // Compute uniform buffer size: base FlowUniformData + dynamic fields (each f32-aligned)
        // Align up to 16-byte boundary for GPU uniform requirements.
        let base_size = std::mem::size_of::<FlowUniformData>() as u64;
        let dynamic_size = (dynamic_field_count * 4) as u64;
        #[allow(clippy::manual_div_ceil)]
        let uniform_size = ((base_size + dynamic_size + 15) / 16) * 16;
        let uniform_size = uniform_size.max(256);

        // Create uniform buffer
        let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("Flow Uniforms: {}", graph.name)),
            size: uniform_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Collect storage buffer bindings
        let mut storage_buffers = HashMap::new();
        let mut storage_entries = Vec::new();
        let mut binding_index = 1u32; // binding(0) is uniforms

        for input in &graph.inputs {
            if let FlowInputSource::Buffer { name, ty } = &input.source {
                if !storage_buffers.contains_key(name) {
                    let elem_size = match ty {
                        FlowType::Float => 4,
                        FlowType::Vec2 => 8,
                        FlowType::Vec3 => 12,
                        FlowType::Vec4 => 16,
                        FlowType::Mat4 => 64,
                    };
                    let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some(&format!("Flow Buffer: {}", name)),
                        size: elem_size * DEFAULT_BUFFER_ELEMENTS,
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    storage_buffers.insert(name.clone(), buf);
                    storage_entries.push((binding_index, name.clone(), false));
                    binding_index += 1;
                }
            }
        }

        // Check for buffer outputs that need read_write access
        for output in &graph.outputs {
            if let blinc_core::FlowOutputTarget::Buffer { name } = &output.target {
                if !storage_buffers.contains_key(name) {
                    let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some(&format!("Flow Buffer: {}", name)),
                        size: 16 * DEFAULT_BUFFER_ELEMENTS, // vec4
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    storage_buffers.insert(name.clone(), buf);
                    storage_entries.push((binding_index, name.clone(), true));
                    binding_index += 1;
                } else {
                    // Promote existing buffer to read_write
                    for entry in &mut storage_entries {
                        if entry.1 == *name {
                            entry.2 = true;
                        }
                    }
                }
            }
        }

        // Build bind group layout entries
        let mut layout_entries = vec![
            // Binding 0: Uniforms
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: match graph.target {
                    FlowTarget::Fragment | FlowTarget::Vertex | FlowTarget::Material => {
                        wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT
                    }
                    FlowTarget::Compute => wgpu::ShaderStages::COMPUTE,
                },
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];

        // Add storage buffer entries
        for (binding, _name, is_rw) in &storage_entries {
            layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: *binding,
                visibility: match graph.target {
                    FlowTarget::Fragment | FlowTarget::Material => wgpu::ShaderStages::FRAGMENT,
                    FlowTarget::Vertex => wgpu::ShaderStages::VERTEX,
                    FlowTarget::Compute => wgpu::ShaderStages::COMPUTE,
                },
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: !*is_rw },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            });
        }

        // Check if this flow uses sample_scene() and needs a scene texture
        let needs_scene = flow_needs_scene_texture(graph);
        let scene_texture_binding = binding_index;
        if needs_scene {
            // Scene texture
            layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: binding_index,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
            binding_index += 1;
            // Scene sampler
            layout_entries.push(wgpu::BindGroupLayoutEntry {
                binding: binding_index,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
            binding_index += 1;
        }
        let _ = binding_index;

        let bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some(&format!("Flow Bind Group Layout: {}", graph.name)),
                    entries: &layout_entries,
                });

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&format!("Flow Pipeline Layout: {}", graph.name)),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let mut render_pipeline = None;
        let mut compute_pipeline = None;

        match graph.target {
            FlowTarget::Fragment => {
                let blend_state = wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                };

                let rp = self
                    .device
                    .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some(&format!("Flow Render Pipeline: {}", graph.name)),
                        layout: Some(&pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: &shader,
                            entry_point: Some("vs_main"),
                            buffers: &[],
                            compilation_options: wgpu::PipelineCompilationOptions::default(),
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &shader,
                            entry_point: Some("fs_main"),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: self.texture_format,
                                blend: Some(blend_state),
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                            compilation_options: wgpu::PipelineCompilationOptions::default(),
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
                    });

                render_pipeline = Some(rp);
            }
            FlowTarget::Compute => {
                let cp = self
                    .device
                    .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some(&format!("Flow Compute Pipeline: {}", graph.name)),
                        layout: Some(&pipeline_layout),
                        module: &shader,
                        entry_point: Some("cs_main"),
                        compilation_options: Default::default(),
                        cache: None,
                    });

                compute_pipeline = Some(cp);
            }
            FlowTarget::Vertex | FlowTarget::Material => {
                // 3D mesh vertex layout (matches blinc_core::Vertex)
                let vertex_stride = std::mem::size_of::<blinc_core::draw::Vertex>() as u64;
                let vertex_layout = wgpu::VertexBufferLayout {
                    array_stride: vertex_stride,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        }, // position
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 12,
                            shader_location: 1,
                        }, // normal
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 2,
                        }, // uv
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 32,
                            shader_location: 3,
                        }, // color
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 48,
                            shader_location: 4,
                        }, // tangent
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32x4,
                            offset: 64,
                            shader_location: 5,
                        }, // joints
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 80,
                            shader_location: 6,
                        }, // weights
                    ],
                };

                let rp = self
                    .device
                    .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some(&format!("Flow 3D Pipeline: {}", graph.name)),
                        layout: Some(&pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: &shader,
                            entry_point: Some("vs_main"),
                            buffers: &[vertex_layout],
                            compilation_options: wgpu::PipelineCompilationOptions::default(),
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &shader,
                            entry_point: Some("fs_main"),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: self.texture_format,
                                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                            compilation_options: wgpu::PipelineCompilationOptions::default(),
                        }),
                        primitive: wgpu::PrimitiveState {
                            topology: wgpu::PrimitiveTopology::TriangleList,
                            cull_mode: Some(wgpu::Face::Back),
                            ..Default::default()
                        },
                        depth_stencil: None,
                        multisample: wgpu::MultisampleState::default(),
                        multiview: None,
                        cache: None,
                    });

                render_pipeline = Some(rp);
            }
        }

        self.touch_lru(&graph.name);
        self.pipelines.insert(
            graph.name.clone(),
            CachedFlowPipeline {
                wgsl_source: wgsl,
                render_pipeline,
                compute_pipeline,
                bind_group_layout,
                uniform_buffer,
                storage_buffers,
                target: graph.target,
                uniform_size,
                needs_scene,
                scene_texture_binding,
                cached_bind_group: None,
                cached_scene_id: 0,
            },
        );

        Ok(())
    }

    /// Invalidate (remove) a cached pipeline, forcing recompilation on next use.
    pub fn invalidate(&mut self, flow_name: &str) {
        self.pipelines.remove(flow_name);
        self.lru_order.retain(|n| n != flow_name);
    }

    /// Invalidate all cached pipelines.
    pub fn invalidate_all(&mut self) {
        self.pipelines.clear();
        self.lru_order.clear();
    }

    /// Build or return the cached bind group for a flow.
    ///
    /// The bind group is rebuilt only when:
    /// - It hasn't been created yet
    /// - The scene texture changed (for flows using sample_scene())
    fn ensure_bind_group(
        &mut self,
        flow_name: &str,
        scene_texture: Option<&wgpu::TextureView>,
    ) -> bool {
        let cached = match self.pipelines.get(flow_name) {
            Some(c) => c,
            None => return false,
        };

        // Determine if we need to rebuild the bind group
        let needs_rebuild = match &cached.cached_bind_group {
            None => true,
            Some(_) => {
                // For scene flows, check if the scene texture generation changed
                cached.needs_scene && cached.cached_scene_id != self.scene_generation
            }
        };

        if !needs_rebuild {
            return true;
        }

        // Build entries
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: cached.uniform_buffer.as_entire_binding(),
        }];

        let mut buf_names: Vec<&String> = cached.storage_buffers.keys().collect();
        buf_names.sort();
        let mut binding = 1u32;
        for name in buf_names {
            if let Some(buf) = cached.storage_buffers.get(name.as_str()) {
                entries.push(wgpu::BindGroupEntry {
                    binding,
                    resource: buf.as_entire_binding(),
                });
                binding += 1;
            }
        }

        if cached.needs_scene {
            let tex_view = scene_texture.unwrap_or(&self.dummy_scene_view);
            entries.push(wgpu::BindGroupEntry {
                binding: cached.scene_texture_binding,
                resource: wgpu::BindingResource::TextureView(tex_view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: cached.scene_texture_binding + 1,
                resource: wgpu::BindingResource::Sampler(&self.scene_sampler),
            });
        }

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("Flow Bind Group: {}", flow_name)),
            layout: &cached.bind_group_layout,
            entries: &entries,
        });

        let gen = self.scene_generation;
        let cached = self.pipelines.get_mut(flow_name).unwrap();
        cached.cached_bind_group = Some(bind_group);
        cached.cached_scene_id = gen;
        true
    }

    /// Update the uniform buffer for a flow and return whether the bind group is ready.
    ///
    /// `scene_texture` is an optional texture view of the current framebuffer,
    /// needed for flows that use `sample_scene()`. If None and the flow needs it,
    /// a 1x1 dummy texture is used.
    ///
    /// Returns false if the flow is not compiled.
    pub fn prepare_render(
        &mut self,
        queue: &wgpu::Queue,
        flow_name: &str,
        uniforms: &FlowUniformData,
        scene_texture: Option<&wgpu::TextureView>,
    ) -> bool {
        let cached = match self.pipelines.get(flow_name) {
            Some(c) => c,
            None => return false,
        };

        // Write uniforms (only CPU→GPU data that changes per-frame)
        queue.write_buffer(&cached.uniform_buffer, 0, bytemuck::bytes_of(uniforms));

        // Ensure bind group is built/cached
        self.ensure_bind_group(flow_name, scene_texture);

        self.touch_lru(flow_name);
        true
    }

    /// Check if a compiled flow needs a scene texture for rendering.
    pub fn needs_scene_texture(&self, flow_name: &str) -> bool {
        self.pipelines.get(flow_name).is_some_and(|c| c.needs_scene)
    }

    /// Check if any compiled flow needs a scene texture.
    pub fn any_needs_scene_texture(&self) -> bool {
        self.pipelines.values().any(|c| c.needs_scene)
    }

    /// Render a fragment flow into a render pass.
    ///
    /// The caller must have already called `prepare_render()`.
    /// Draws a fullscreen quad (6 vertices from the flow's vertex shader).
    pub fn render_fragment<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, flow_name: &str) -> bool {
        let cached = match self.pipelines.get(flow_name) {
            Some(c) => c,
            None => return false,
        };

        let pipeline = match &cached.render_pipeline {
            Some(p) => p,
            None => return false,
        };

        let bind_group = match &cached.cached_bind_group {
            Some(bg) => bg,
            None => return false,
        };

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..6, 0..1);
        true
    }

    /// Dispatch a compute flow.
    ///
    /// The caller must have already called `prepare_render()`.
    pub fn dispatch_compute<'a>(
        &'a self,
        pass: &mut wgpu::ComputePass<'a>,
        flow_name: &str,
        workgroup_count: u32,
    ) -> bool {
        let cached = match self.pipelines.get(flow_name) {
            Some(c) => c,
            None => return false,
        };

        let pipeline = match &cached.compute_pipeline {
            Some(p) => p,
            None => return false,
        };

        let bind_group = match &cached.cached_bind_group {
            Some(bg) => bg,
            None => return false,
        };

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.dispatch_workgroups(workgroup_count, 1, 1);
        true
    }

    /// Get the target type of a compiled flow
    pub fn flow_target(&self, flow_name: &str) -> Option<FlowTarget> {
        self.pipelines.get(flow_name).map(|c| c.target)
    }

    /// Get the number of compiled pipelines
    pub fn len(&self) -> usize {
        self.pipelines.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.pipelines.is_empty()
    }
}

// ==========================================================================
// Tests
// ==========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flow_uniform_data_size() {
        // Ensure the uniform struct is properly aligned for GPU upload
        let size = std::mem::size_of::<FlowUniformData>();
        assert_eq!(size, 48); // 12 f32s * 4 bytes = 48 bytes
        assert_eq!(size % 4, 0); // Must be f32-aligned
    }

    #[test]
    fn test_flow_uniform_data_default() {
        let data = FlowUniformData::default();
        assert_eq!(data.time, 0.0);
        assert_eq!(data.frame_index, 0.0);
        assert_eq!(data.viewport_size, [0.0; 2]);
    }
}
