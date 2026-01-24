//! GPU SDF Raymarching Renderer
//!
//! Provides GPU-accelerated rendering of SDF scenes using wgpu.

use std::collections::HashMap;
use std::sync::Arc;

use super::{SdfScene, SdfUniform};
use crate::sdf::codegen::SdfCodegen;

/// SDF Renderer that uses GPU raymarching
pub struct SdfGpuRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface_format: wgpu::TextureFormat,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    uniform_bind_group: wgpu::BindGroup,
    /// Cache of compiled pipelines, keyed by shader hash
    pipeline_cache: HashMap<u64, wgpu::RenderPipeline>,
}

impl SdfGpuRenderer {
    /// Create a new SDF GPU renderer
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        // Create uniform buffer
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SDF Uniform Buffer"),
            size: std::mem::size_of::<SdfUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group layout
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("SDF Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // Create bind group
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Self {
            device,
            queue,
            surface_format,
            uniform_buffer,
            uniform_bind_group_layout,
            uniform_bind_group,
            pipeline_cache: HashMap::new(),
        }
    }

    /// Get or create a render pipeline for the given SDF scene
    fn get_or_create_pipeline(&mut self, scene: &SdfScene) -> &wgpu::RenderPipeline {
        // Generate shader code
        let shader_code = SdfCodegen::generate_full_shader(scene);

        // Hash the shader for caching
        let shader_hash = Self::hash_string(&shader_code);

        // Check cache
        if !self.pipeline_cache.contains_key(&shader_hash) {
            // Create shader module
            let shader_module = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("SDF Raymarch Shader"),
                    source: wgpu::ShaderSource::Wgsl(shader_code.into()),
                });

            // Create pipeline layout
            let pipeline_layout =
                self.device
                    .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("SDF Pipeline Layout"),
                        bind_group_layouts: &[&self.uniform_bind_group_layout],
                        push_constant_ranges: &[],
                    });

            // Create render pipeline
            let pipeline = self
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("SDF Raymarch Pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader_module,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader_module,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: self.surface_format,
                            blend: Some(wgpu::BlendState::REPLACE),
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
                });

            self.pipeline_cache.insert(shader_hash, pipeline);
        }

        self.pipeline_cache.get(&shader_hash).unwrap()
    }

    /// Simple string hash for caching
    fn hash_string(s: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }

    /// Render an SDF scene to a render target
    ///
    /// # Arguments
    /// * `scene` - The SDF scene to render
    /// * `uniforms` - Camera and render settings
    /// * `render_target` - The texture view to render to
    pub fn render(
        &mut self,
        scene: &SdfScene,
        uniforms: &SdfUniform,
        render_target: &wgpu::TextureView,
    ) {
        // Update uniforms
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));

        // Get or create pipeline (this borrows self mutably)
        // We need to get a raw pointer to the pipeline to avoid borrow issues
        let shader_code = SdfCodegen::generate_full_shader(scene);
        let shader_hash = Self::hash_string(&shader_code);

        // Check cache and create pipeline if needed
        if !self.pipeline_cache.contains_key(&shader_hash) {
            let shader_module = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("SDF Raymarch Shader"),
                    source: wgpu::ShaderSource::Wgsl(shader_code.into()),
                });

            let pipeline_layout =
                self.device
                    .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("SDF Pipeline Layout"),
                        bind_group_layouts: &[&self.uniform_bind_group_layout],
                        push_constant_ranges: &[],
                    });

            let pipeline = self
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("SDF Raymarch Pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader_module,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader_module,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: self.surface_format,
                            blend: Some(wgpu::BlendState::REPLACE),
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
                });

            self.pipeline_cache.insert(shader_hash, pipeline);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("SDF Render Encoder"),
            });

        // Render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SDF Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: render_target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.15,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let pipeline = self.pipeline_cache.get(&shader_hash).unwrap();
            render_pass.set_pipeline(pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.draw(0..3, 0..1); // Fullscreen triangle
        }

        // Submit
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render to a new texture and return it
    ///
    /// Creates a texture of the specified size, renders the SDF scene to it,
    /// and returns the texture.
    pub fn render_to_texture(
        &mut self,
        scene: &SdfScene,
        uniforms: &SdfUniform,
        width: u32,
        height: u32,
    ) -> wgpu::Texture {
        // Create texture
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SDF Render Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.render(scene, uniforms, &view);

        texture
    }

    /// Clear the pipeline cache
    pub fn clear_cache(&mut self) {
        self.pipeline_cache.clear();
    }
}

/// Camera parameters for SDF rendering
#[derive(Clone, Debug)]
pub struct SdfCamera {
    pub position: blinc_core::Vec3,
    pub target: blinc_core::Vec3,
    pub up: blinc_core::Vec3,
    pub fov: f32, // Field of view in radians
}

impl Default for SdfCamera {
    fn default() -> Self {
        Self {
            position: blinc_core::Vec3::new(0.0, 2.0, 5.0),
            target: blinc_core::Vec3::ZERO,
            up: blinc_core::Vec3::new(0.0, 1.0, 0.0),
            fov: 0.8,
        }
    }
}

impl SdfCamera {
    /// Convert to SdfUniform with resolution and time
    pub fn to_uniform(&self, width: f32, height: f32, time: f32) -> SdfUniform {
        // Manual Vec3 subtraction (target - position)
        let direction = blinc_core::Vec3::new(
            self.target.x - self.position.x,
            self.target.y - self.position.y,
            self.target.z - self.position.z,
        );
        let dir_len = (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
        let direction = blinc_core::Vec3::new(
            direction.x / dir_len,
            direction.y / dir_len,
            direction.z / dir_len,
        );

        // Calculate right vector (cross product of direction and up)
        let right = blinc_core::Vec3::new(
            direction.z * self.up.y - direction.y * self.up.z,
            direction.x * self.up.z - direction.z * self.up.x,
            direction.y * self.up.x - direction.x * self.up.y,
        );
        let right_len = (right.x * right.x + right.y * right.y + right.z * right.z).sqrt();
        let right = blinc_core::Vec3::new(right.x / right_len, right.y / right_len, right.z / right_len);

        // Recalculate up (cross product of right and direction)
        let up = blinc_core::Vec3::new(
            right.y * direction.z - right.z * direction.y,
            right.z * direction.x - right.x * direction.z,
            right.x * direction.y - right.y * direction.x,
        );

        SdfUniform {
            camera_pos: [self.position.x, self.position.y, self.position.z, 1.0],
            camera_dir: [direction.x, direction.y, direction.z, 0.0],
            camera_up: [up.x, up.y, up.z, 0.0],
            camera_right: [right.x, right.y, right.z, 0.0],
            resolution: [width, height],
            time,
            fov: self.fov,
            max_steps: 128,
            max_distance: 100.0,
            epsilon: 0.001,
            _padding: 0.0,
        }
    }
}
