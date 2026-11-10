//! GPU Particle System
//!
//! Provides fully GPU-accelerated particle simulation and rendering.
//! Particles are simulated using compute shaders and rendered using
//! instanced billboards.
//!
//! # Architecture
//!
//! ```text
//! ParticleSystemGpu
//!        │
//!        ├── Compute Pass (Simulation)
//!        │   └── Updates particle positions, velocities, lifetimes
//!        │
//!        └── Render Pass (Drawing)
//!            └── Draws particle billboards as instanced quads
//! ```

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

/// Maximum particles per system for buffer allocation
pub const MAX_PARTICLES_PER_SYSTEM: u32 = 100_000;

/// GPU particle data structure
/// Must match the WGSL struct layout exactly
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuParticle {
    /// Position (xyz) and life remaining (w)
    pub position_life: [f32; 4],
    /// Velocity (xyz) and max lifetime (w)
    pub velocity_max_life: [f32; 4],
    /// Color (rgba)
    pub color: [f32; 4],
    /// Size (current, start, end, rotation)
    pub size_rotation: [f32; 4],
}

impl Default for GpuParticle {
    fn default() -> Self {
        Self {
            position_life: [0.0, 0.0, 0.0, 0.0], // life=0 means inactive
            velocity_max_life: [0.0, 0.0, 0.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            size_rotation: [0.1, 0.1, 0.0, 0.0],
        }
    }
}

/// Emitter configuration for GPU
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuEmitter {
    /// Emitter position (xyz) and shape type (w as u32 bits)
    pub position_shape: [f32; 4],
    /// Shape parameters (radius, angle, half_extents, etc.)
    pub shape_params: [f32; 4],
    /// Direction (xyz) and randomness (w)
    pub direction_randomness: [f32; 4],
    /// Emission rate, burst count, spawn accumulated, gravity scale
    pub emission_config: [f32; 4],
    /// Lifetime (min, max), speed (min, max)
    pub lifetime_speed: [f32; 4],
    /// Start size (min, max), end size (min, max)
    pub size_config: [f32; 4],
    /// Start color (rgba) - base of fire (yellow)
    pub start_color: [f32; 4],
    /// Mid color (rgba) - middle of fire (red)
    pub mid_color: [f32; 4],
    /// End color (rgba) - tip of fire (dark/burnt)
    pub end_color: [f32; 4],
}

impl Default for GpuEmitter {
    fn default() -> Self {
        Self {
            position_shape: [0.0, 0.0, 0.0, 0.0], // Point emitter
            shape_params: [0.0; 4],
            direction_randomness: [0.0, 1.0, 0.0, 0.0], // Up, no randomness
            emission_config: [100.0, 0.0, 0.0, 1.0],    // 100/s, no burst, gravity=1
            lifetime_speed: [1.0, 2.0, 1.0, 2.0],
            size_config: [0.1, 0.2, 0.0, 0.1],
            start_color: [1.0, 1.0, 1.0, 1.0],
            mid_color: [1.0, 1.0, 1.0, 0.5],
            end_color: [1.0, 1.0, 1.0, 0.0],
        }
    }
}

/// Force affector for GPU
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuForce {
    /// Force type (0=gravity, 1=wind, 2=vortex, 3=drag, 4=turbulence, 5=attractor)
    /// and strength packed as (type, strength, 0, 0)
    pub type_strength: [f32; 4],
    /// Direction/position (xyz) and extra param (w)
    pub direction_params: [f32; 4],
}

impl Default for GpuForce {
    fn default() -> Self {
        Self {
            type_strength: [0.0, 0.0, 0.0, 0.0],
            direction_params: [0.0, -9.8, 0.0, 0.0], // Default gravity
        }
    }
}

/// Simulation uniforms for compute shader
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuSimulationUniforms {
    /// Delta time, total time, max particles, active particles
    pub time_config: [f32; 4],
    /// Random seed (4 values for better distribution)
    pub random_seed: [f32; 4],
    /// Number of forces, padding
    pub force_config: [f32; 4],
}

impl Default for GpuSimulationUniforms {
    fn default() -> Self {
        Self {
            time_config: [0.016, 0.0, MAX_PARTICLES_PER_SYSTEM as f32, 0.0],
            random_seed: [0.0; 4],
            force_config: [0.0; 4],
        }
    }
}

/// Render uniforms for vertex/fragment shaders
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuRenderUniforms {
    /// View-projection matrix (column-major)
    pub view_proj: [[f32; 4]; 4],
    /// Camera position (xyz) and FOV (w)
    pub camera_pos_fov: [f32; 4],
    /// Camera right vector (xyz) and aspect ratio (w)
    pub camera_right_aspect: [f32; 4],
    /// Camera up vector (xyz) and padding (w)
    pub camera_up: [f32; 4],
    /// Viewport size (width, height) and render mode, blend mode
    pub viewport_config: [f32; 4],
}

impl Default for GpuRenderUniforms {
    fn default() -> Self {
        Self {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            camera_pos_fov: [0.0, 0.0, 5.0, 0.8],
            camera_right_aspect: [1.0, 0.0, 0.0, 1.0],
            camera_up: [0.0, 1.0, 0.0, 0.0],
            viewport_config: [800.0, 600.0, 0.0, 0.0],
        }
    }
}

/// Particle viewport data for rendering
#[derive(Clone, Debug)]
pub struct ParticleViewport {
    /// Emitter configuration
    pub emitter: GpuEmitter,
    /// Force affectors (up to 8)
    pub forces: Vec<GpuForce>,
    /// Maximum particles
    pub max_particles: u32,
    /// Camera position
    pub camera_pos: [f32; 3],
    /// Camera target
    pub camera_target: [f32; 3],
    /// Camera up vector
    pub camera_up: [f32; 3],
    /// Field of view
    pub fov: f32,
    /// Current time
    pub time: f32,
    /// Delta time
    pub delta_time: f32,
    /// Viewport bounds (x, y, width, height)
    pub bounds: [f32; 4],
    /// Blend mode (0=alpha, 1=additive)
    pub blend_mode: u32,
    /// Whether system is playing
    pub playing: bool,
}

impl Default for ParticleViewport {
    fn default() -> Self {
        Self {
            emitter: GpuEmitter::default(),
            forces: Vec::new(),
            max_particles: 10000,
            camera_pos: [0.0, 0.0, 5.0],
            camera_target: [0.0, 0.0, 0.0],
            camera_up: [0.0, 1.0, 0.0],
            fov: 0.8,
            time: 0.0,
            delta_time: 0.016,
            bounds: [0.0, 0.0, 800.0, 600.0],
            blend_mode: 0,
            playing: true,
        }
    }
}

/// GPU particle compute shader
pub const PARTICLE_COMPUTE_SHADER: &str = crate::sdf_russl::particle_compute::WGSL;

/// Data-texture fallback: Particle rendering (no storage buffers — WebGL2)
pub const PARTICLE_RENDER_DT_SHADER: &str = crate::sdf_russl::particle_render_dt::WGSL;

/// GPU particle render shader (billboard quads)
pub const PARTICLE_RENDER_SHADER: &str = crate::sdf_russl::particle_render::WGSL;

/// Handle to a GPU particle system
#[derive(Debug)]
pub struct ParticleSystemGpu {
    /// Particle buffer (read/write for compute)
    particle_buffer: wgpu::Buffer,
    /// Emitter uniform buffer
    emitter_buffer: wgpu::Buffer,
    /// Simulation uniforms buffer
    sim_uniform_buffer: wgpu::Buffer,
    /// Render uniforms buffer
    render_uniform_buffer: wgpu::Buffer,
    /// Forces buffer
    forces_buffer: wgpu::Buffer,
    /// Compute pipeline
    compute_pipeline: wgpu::ComputePipeline,
    /// Render pipeline
    render_pipeline: wgpu::RenderPipeline,
    /// Compute bind group
    compute_bind_group: wgpu::BindGroup,
    /// Render bind group
    render_bind_group: wgpu::BindGroup,
    /// Max particles
    max_particles: u32,
    /// Current time
    time: f32,
}

impl ParticleSystemGpu {
    /// Create a new GPU particle system
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        max_particles: u32,
    ) -> Self {
        let max_particles = max_particles.min(MAX_PARTICLES_PER_SYSTEM);

        // Create particle buffer
        let particle_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Buffer"),
            size: (std::mem::size_of::<GpuParticle>() * max_particles as usize) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create emitter buffer
        let emitter_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Emitter Buffer"),
            size: std::mem::size_of::<GpuEmitter>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create simulation uniforms buffer
        let sim_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Simulation Uniforms"),
            size: std::mem::size_of::<GpuSimulationUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create render uniforms buffer
        let render_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Render Uniforms"),
            size: std::mem::size_of::<GpuRenderUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create forces buffer (support up to 8 forces)
        let forces_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Forces Buffer"),
            size: (std::mem::size_of::<GpuForce>() * 8) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create compute shader module
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particle Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(PARTICLE_COMPUTE_SHADER.into()),
        });

        // Create render shader module
        let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particle Render Shader"),
            source: wgpu::ShaderSource::Wgsl(PARTICLE_RENDER_SHADER.into()),
        });

        // Create compute bind group layout
        let compute_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Particle Compute Bind Group Layout"),
                entries: &[
                    // Particles (storage, read_write)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Emitter (uniform)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Simulation uniforms
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Forces (storage, read)
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        // Create render bind group layout
        let render_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Particle Render Bind Group Layout"),
                entries: &[
                    // Particles (storage, read)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Render uniforms
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        // Create compute pipeline
        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Particle Compute Pipeline Layout"),
                bind_group_layouts: &[&compute_bind_group_layout],
                push_constant_ranges: &[],
            });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Particle Compute Pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // Create render pipeline
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Particle Render Pipeline Layout"),
                bind_group_layouts: &[&render_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Particle Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &render_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &render_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
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
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // Billboards face camera
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Create bind groups
        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Particle Compute Bind Group"),
            layout: &compute_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: particle_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: emitter_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: sim_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: forces_buffer.as_entire_binding(),
                },
            ],
        });

        let render_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Particle Render Bind Group"),
            layout: &render_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: particle_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: render_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            particle_buffer,
            emitter_buffer,
            sim_uniform_buffer,
            render_uniform_buffer,
            forces_buffer,
            compute_pipeline,
            render_pipeline,
            compute_bind_group,
            render_bind_group,
            max_particles,
            time: 0.0,
        }
    }

    /// Update the particle system for one frame
    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        viewport: &ParticleViewport,
    ) {
        if !viewport.playing {
            return;
        }

        self.time = viewport.time;

        // Update emitter buffer
        queue.write_buffer(
            &self.emitter_buffer,
            0,
            bytemuck::bytes_of(&viewport.emitter),
        );

        // Update simulation uniforms
        let sim_uniforms = GpuSimulationUniforms {
            time_config: [
                viewport.delta_time,
                viewport.time,
                self.max_particles as f32,
                0.0,
            ],
            random_seed: [
                viewport.time * 12_345.679,
                viewport.time * 98_765.43,
                (viewport.time * 11_111.111).fract(),
                (viewport.time * 22_222.223).fract(),
            ],
            force_config: [viewport.forces.len() as f32, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(
            &self.sim_uniform_buffer,
            0,
            bytemuck::bytes_of(&sim_uniforms),
        );

        // Update forces buffer
        let mut forces = [GpuForce::default(); 8];
        for (i, force) in viewport.forces.iter().take(8).enumerate() {
            forces[i] = *force;
        }
        queue.write_buffer(&self.forces_buffer, 0, bytemuck::cast_slice(&forces));

        // Dispatch compute shader
        let workgroups = self.max_particles.div_ceil(64);
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Particle Compute Pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&self.compute_pipeline);
        compute_pass.set_bind_group(0, &self.compute_bind_group, &[]);
        compute_pass.dispatch_workgroups(workgroups, 1, 1);
    }

    /// Render the particles
    pub fn render<'a>(
        &'a self,
        queue: &wgpu::Queue,
        render_pass: &mut wgpu::RenderPass<'a>,
        viewport: &ParticleViewport,
    ) {
        // Calculate view-projection matrix
        let view = Self::look_at(
            viewport.camera_pos,
            viewport.camera_target,
            viewport.camera_up,
        );
        let aspect = viewport.bounds[2] / viewport.bounds[3];
        let proj = Self::perspective(viewport.fov, aspect, 0.1, 100.0);
        let view_proj = Self::mat4_mul(&proj, &view);

        // Calculate camera vectors
        let forward = [
            viewport.camera_target[0] - viewport.camera_pos[0],
            viewport.camera_target[1] - viewport.camera_pos[1],
            viewport.camera_target[2] - viewport.camera_pos[2],
        ];
        let forward = Self::normalize(forward);
        let right = Self::cross(forward, viewport.camera_up);
        let right = Self::normalize(right);
        let up = Self::cross(right, forward);

        // Update render uniforms
        let render_uniforms = GpuRenderUniforms {
            view_proj,
            camera_pos_fov: [
                viewport.camera_pos[0],
                viewport.camera_pos[1],
                viewport.camera_pos[2],
                viewport.fov,
            ],
            camera_right_aspect: [right[0], right[1], right[2], aspect],
            camera_up: [up[0], up[1], up[2], 0.0],
            viewport_config: [
                viewport.bounds[2],
                viewport.bounds[3],
                0.0, // render mode
                viewport.blend_mode as f32,
            ],
        };
        queue.write_buffer(
            &self.render_uniform_buffer,
            0,
            bytemuck::bytes_of(&render_uniforms),
        );

        // Draw particles
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.render_bind_group, &[]);
        render_pass.draw(0..6, 0..self.max_particles); // 6 vertices per quad (2 triangles)
    }

    // Helper math functions
    fn normalize(v: [f32; 3]) -> [f32; 3] {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len > 0.0001 {
            [v[0] / len, v[1] / len, v[2] / len]
        } else {
            [0.0, 1.0, 0.0]
        }
    }

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> [[f32; 4]; 4] {
        let f = Self::normalize([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]]);
        let r = Self::normalize(Self::cross(f, up));
        let u = Self::cross(r, f);

        [
            [r[0], u[0], -f[0], 0.0],
            [r[1], u[1], -f[1], 0.0],
            [r[2], u[2], -f[2], 0.0],
            [
                -(r[0] * eye[0] + r[1] * eye[1] + r[2] * eye[2]),
                -(u[0] * eye[0] + u[1] * eye[1] + u[2] * eye[2]),
                f[0] * eye[0] + f[1] * eye[1] + f[2] * eye[2],
                1.0,
            ],
        ]
    }

    fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
        let f = 1.0 / (fov * 0.5).tan();
        let nf = 1.0 / (near - far);

        [
            [f / aspect, 0.0, 0.0, 0.0],
            [0.0, f, 0.0, 0.0],
            [0.0, 0.0, (far + near) * nf, -1.0],
            [0.0, 0.0, 2.0 * far * near * nf, 0.0],
        ]
    }

    fn mat4_mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
        let mut result = [[0.0f32; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                result[i][j] =
                    a[0][j] * b[i][0] + a[1][j] * b[i][1] + a[2][j] * b[i][2] + a[3][j] * b[i][3];
            }
        }
        result
    }
}

/// Manager for multiple particle systems
pub struct ParticleManager {
    systems: HashMap<u64, ParticleSystemGpu>,
    next_id: u64,
}

impl ParticleManager {
    pub fn new() -> Self {
        Self {
            systems: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn create_system(
        &mut self,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        max_particles: u32,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let system = ParticleSystemGpu::new(device, surface_format, max_particles);
        self.systems.insert(id, system);
        id
    }

    pub fn get_system(&self, id: u64) -> Option<&ParticleSystemGpu> {
        self.systems.get(&id)
    }

    pub fn get_system_mut(&mut self, id: u64) -> Option<&mut ParticleSystemGpu> {
        self.systems.get_mut(&id)
    }

    pub fn remove_system(&mut self, id: u64) {
        self.systems.remove(&id);
    }
}

impl Default for ParticleManager {
    fn default() -> Self {
        Self::new()
    }
}
