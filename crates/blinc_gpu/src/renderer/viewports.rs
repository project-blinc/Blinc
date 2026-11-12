//! Custom passes, SDF 3D viewports and particle viewports.

use super::{GpuRenderer, Sdf3DResources};
use crate::primitives::{Sdf3DUniform, Viewport3D};
use std::collections::HashMap;

impl GpuRenderer {
    /// Register a custom render pass.
    ///
    /// The pass will be initialized immediately and executed each frame
    /// at the stage returned by `pass.stage()`.
    pub fn register_custom_pass(
        &mut self,
        mut pass: Box<dyn crate::custom_pass::CustomRenderPass>,
    ) {
        pass.initialize(&self.device, &self.queue, self.texture_format);
        self.custom_passes.register(pass);
    }

    /// Remove a custom render pass by label.
    pub fn remove_custom_pass(&mut self, label: &str) -> bool {
        self.custom_passes.remove(label)
    }

    /// Execute all custom passes for a given stage.
    pub fn execute_custom_passes(
        &mut self,
        stage: crate::custom_pass::RenderStage,
        target: &wgpu::TextureView,
        scale_factor: f64,
    ) {
        if !self.custom_passes.has_passes(stage) {
            return;
        }
        let ctx = crate::custom_pass::RenderPassContext {
            device: &self.device,
            queue: &self.queue,
            target,
            viewport_width: self.viewport_size.0,
            viewport_height: self.viewport_size.1,
            texture_format: self.texture_format,
            scale_factor,
            view_proj: None,
            inv_view_proj: None,
            camera_pos: None,
            viewport: None,
        };
        self.custom_passes.execute_stage(stage, &ctx);
    }

    /// Execute Scene3D custom passes with camera context.
    pub fn execute_scene3d_passes(
        &mut self,
        target: &wgpu::TextureView,
        scale_factor: f64,
        view_proj: &[f32; 16],
        inv_view_proj: &[f32; 16],
        camera_pos: [f32; 3],
        viewport: Option<[f32; 4]>,
    ) {
        let stage = crate::custom_pass::RenderStage::Scene3D;
        if !self.custom_passes.has_passes(stage) {
            return;
        }
        let ctx = crate::custom_pass::RenderPassContext {
            device: &self.device,
            queue: &self.queue,
            target,
            viewport_width: self.viewport_size.0,
            viewport_height: self.viewport_size.1,
            texture_format: self.texture_format,
            scale_factor,
            view_proj: Some(*view_proj),
            inv_view_proj: Some(*inv_view_proj),
            camera_pos: Some(camera_pos),
            viewport,
        };
        self.custom_passes.execute_stage(stage, &ctx);
    }

    /// Notify custom passes of a viewport resize.
    pub fn resize_custom_passes(&mut self, width: u32, height: u32) {
        self.custom_passes.resize(&self.device, width, height);
    }

    // ─── GPU Memory Budget ─────────────────────────────────────────────────

    /// Initialize SDF 3D resources lazily
    pub(super) fn ensure_sdf_3d_resources(&mut self) {
        if self.sdf_3d_resources.is_some() {
            return;
        }

        // Create bind group layout for SDF 3D uniforms
        let bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("SDF 3D Bind Group Layout"),
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

        // Create uniform buffer
        let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SDF 3D Uniform Buffer"),
            size: std::mem::size_of::<Sdf3DUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF 3D Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        self.sdf_3d_resources = Some(Sdf3DResources {
            bind_group_layout,
            uniform_buffer,
            bind_group,
            pipeline_cache: HashMap::new(),
        });
    }

    /// Get or create a render pipeline for an SDF 3D viewport
    pub(super) fn get_or_create_sdf_3d_pipeline(&mut self, shader_wgsl: &str) -> u64 {
        self.ensure_sdf_3d_resources();

        // Hash the shader for caching
        let shader_hash = Self::hash_string(shader_wgsl);

        let resources = self.sdf_3d_resources.as_mut().unwrap();

        if !resources.pipeline_cache.contains_key(&shader_hash) {
            // Create shader module
            let shader_module = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("SDF 3D Raymarch Shader"),
                    source: wgpu::ShaderSource::Wgsl(shader_wgsl.into()),
                });

            // Create pipeline layout
            let pipeline_layout =
                self.device
                    .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("SDF 3D Pipeline Layout"),
                        bind_group_layouts: &[&resources.bind_group_layout],
                        push_constant_ranges: &[],
                    });

            // Create render pipeline
            let pipeline = self
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("SDF 3D Raymarch Pipeline"),
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
                            format: self.texture_format,
                            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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

            resources.pipeline_cache.insert(shader_hash, pipeline);
        }

        shader_hash
    }

    /// Simple string hash for shader caching
    pub(super) fn hash_string(s: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }

    /// Render SDF 3D viewports to the target
    pub fn render_sdf_3d_viewports(
        &mut self,
        target: &wgpu::TextureView,
        viewports: &[Viewport3D],
    ) {
        if viewports.is_empty() {
            return;
        }

        self.ensure_sdf_3d_resources();

        let (surface_width, surface_height) = self.viewport_size;

        for viewport in viewports {
            // The paint context already clipped to its clip stack, but we need to
            // further clamp to the render target bounds for wgpu validity.
            // If we need to clamp further, we must also adjust the UV offset/scale.
            let orig_x = viewport.bounds[0];
            let orig_y = viewport.bounds[1];
            let orig_w = viewport.bounds[2];
            let orig_h = viewport.bounds[3];

            // Clamp to render target bounds
            let x = orig_x.max(0.0);
            let y = orig_y.max(0.0);
            let right = (orig_x + orig_w).min(surface_width as f32);
            let bottom = (orig_y + orig_h).min(surface_height as f32);
            let w = (right - x).max(0.0);
            let h = (bottom - y).max(0.0);

            // Skip if viewport is fully outside the render target or has zero size
            if w <= 0.0 || h <= 0.0 {
                continue;
            }

            // Check if we needed to clamp further and adjust UV accordingly
            let mut uniforms = viewport.uniforms;
            if orig_w > 0.0 && orig_h > 0.0 {
                // Calculate additional UV adjustment for surface clamping
                // The paint context's UV maps the paint-clipped region to the original viewport.
                // If we clamp further here, we need to adjust those UVs.
                let extra_offset_x = (x - orig_x) / orig_w;
                let extra_offset_y = (y - orig_y) / orig_h;
                let extra_scale_x = w / orig_w;
                let extra_scale_y = h / orig_h;

                // Compose with existing UV transform: new_uv = old_offset + (extra_offset + uv * extra_scale) * old_scale
                // Which simplifies to: new_offset = old_offset + extra_offset * old_scale, new_scale = old_scale * extra_scale
                uniforms.uv_offset[0] += extra_offset_x * uniforms.uv_scale[0];
                uniforms.uv_offset[1] += extra_offset_y * uniforms.uv_scale[1];
                uniforms.uv_scale[0] *= extra_scale_x;
                uniforms.uv_scale[1] *= extra_scale_y;
            }

            // Get or create pipeline for this viewport's shader
            let shader_hash = self.get_or_create_sdf_3d_pipeline(&viewport.shader_wgsl);

            // Update uniforms with adjusted UV
            let resources = self.sdf_3d_resources.as_ref().unwrap();
            self.queue
                .write_buffer(&resources.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

            // Create command encoder
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("SDF 3D Render Encoder"),
                });

            // Render pass
            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("SDF 3D Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            // Don't clear - we're rendering on top of existing content
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                // Set viewport and scissor to the clamped bounds
                render_pass.set_viewport(x, y, w, h, 0.0, 1.0);
                render_pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);

                let resources = self.sdf_3d_resources.as_ref().unwrap();
                let pipeline = resources.pipeline_cache.get(&shader_hash).unwrap();
                render_pass.set_pipeline(pipeline);
                render_pass.set_bind_group(0, &resources.bind_group, &[]);
                render_pass.draw(0..3, 0..1); // Fullscreen triangle
            }

            // Submit
            self.queue.submit(std::iter::once(encoder.finish()));
        }
    }

    /// Render GPU particle viewports
    pub fn render_particle_viewports(
        &mut self,
        target: &wgpu::TextureView,
        viewports: &[crate::primitives::ParticleViewport3D],
    ) {
        use crate::particles::{ParticleSystemGpu, ParticleViewport};
        use std::hash::{Hash, Hasher};

        if viewports.is_empty() {
            return;
        }

        // Particles require compute shaders (for the simulation pass) and
        // storage buffers (for the particle buffer). WebGL2 has neither,
        // so skip particle rendering entirely in DT/Tier-3 mode.
        if !self.has_storage_buffers {
            return;
        }

        // Use the actual texture format that was selected during renderer initialization
        let surface_format = self.texture_format;

        for (vp_index, vp) in viewports.iter().enumerate() {
            if !vp.playing {
                continue;
            }

            // Generate a stable hash key for this particle system based on emitter config
            // This allows us to reuse the same GPU buffers across frames
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            vp_index.hash(&mut hasher);
            vp.max_particles.hash(&mut hasher);
            // Hash emitter position components to differentiate systems at different positions
            (vp.emitter.position_shape[0].to_bits()).hash(&mut hasher);
            (vp.emitter.position_shape[1].to_bits()).hash(&mut hasher);
            (vp.emitter.position_shape[2].to_bits()).hash(&mut hasher);
            let system_key = hasher.finish();

            // Get or create the particle system
            let system = self.particle_systems.entry(system_key).or_insert_with(|| {
                ParticleSystemGpu::new(&self.device, surface_format, vp.max_particles)
            });

            // Convert ParticleViewport3D to ParticleViewport for the GPU system
            let particle_viewport = ParticleViewport {
                emitter: vp.emitter,
                forces: vp.forces.clone(),
                max_particles: vp.max_particles,
                camera_pos: vp.camera_pos,
                camera_target: vp.camera_target,
                camera_up: vp.camera_up,
                fov: vp.fov,
                time: vp.time,
                delta_time: vp.delta_time,
                bounds: vp.bounds,
                blend_mode: vp.blend_mode,
                playing: vp.playing,
            };

            // Create command encoder
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Particle Encoder"),
                });

            // Run compute pass to update particles
            system.update(&self.queue, &mut encoder, &particle_viewport);

            // Submit compute work first
            self.queue.submit(std::iter::once(encoder.finish()));

            // Create render encoder
            let mut render_encoder =
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Particle Render Encoder"),
                    });

            // Render pass
            {
                let mut render_pass =
                    render_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Particle Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: target,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load, // Don't clear, draw on top
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                // Set viewport to the particle bounds
                render_pass.set_viewport(
                    vp.bounds[0],
                    vp.bounds[1],
                    vp.bounds[2],
                    vp.bounds[3],
                    0.0,
                    1.0,
                );

                // Render the particles
                system.render(&self.queue, &mut render_pass, &particle_viewport);
            }

            // Submit render work
            self.queue.submit(std::iter::once(render_encoder.finish()));
        }
    }
}
