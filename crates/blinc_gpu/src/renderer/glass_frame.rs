//! Glass frames: rendering the backdrop and the glass over it.

use super::GpuRenderer;
use crate::primitives::{GlassType, GpuGlassPrimitive, PrimitiveBatch, Uniforms};
use crate::shaders::{GLASS_DT_SHADER, GLASS_SHADER, SIMPLE_GLASS_DT_SHADER, SIMPLE_GLASS_SHADER};

impl GpuRenderer {
    /// Render glass primitives (requires backdrop texture)
    ///
    /// Splits primitives into simple (frosted) and liquid (refracted) glass,
    /// rendering each with the appropriate shader.
    pub fn render_glass(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        batch: &PrimitiveBatch,
    ) {
        self.render_glass_at(
            target,
            backdrop,
            batch,
            [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            [0.0; 2],
            false,
        );
    }

    pub(super) fn render_glass_at(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        sample_size: [f32; 2],
        origin: [f32; 2],
        clip_in_image: bool,
    ) {
        if batch.glass_primitives.is_empty() {
            return;
        }
        self.ensure_glass_pipelines();

        // Split primitives: simple glass first, then liquid glass
        // This allows us to render each group with its respective pipeline
        let mut simple_primitives: Vec<GpuGlassPrimitive> = Vec::new();
        let mut liquid_primitives: Vec<GpuGlassPrimitive> = Vec::new();

        for prim in &batch.glass_primitives {
            if prim.type_info[0] == GlassType::Simple as u32 {
                simple_primitives.push(*prim);
            } else {
                liquid_primitives.push(*prim);
            }
        }

        let simple_count = simple_primitives.len();
        let liquid_count = liquid_primitives.len();

        if simple_count == 0 && liquid_count == 0 {
            return;
        }

        // Combine: simple primitives first, then liquid primitives
        let mut ordered_primitives = simple_primitives;
        ordered_primitives.extend(liquid_primitives);

        self.update_aux_data_buffer(batch);
        self.prepare_glass_draw_at(
            backdrop,
            &ordered_primitives,
            sample_size,
            origin,
            clip_in_image,
        );
        let glass_bind_group = self
            .cached_glass
            .as_ref()
            .unwrap()
            .bind_group
            .as_ref()
            .unwrap();

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Glass Render Encoder"),
            });

        // Begin render pass (load existing content)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Glass Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Keep existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render simple glass primitives with the simple_glass pipeline
            if simple_count > 0 {
                render_pass.set_pipeline(self.effect_pipelines.simple_glass.as_ref().unwrap());
                render_pass.set_bind_group(0, glass_bind_group, &[]);
                render_pass.draw(0..6, 0..simple_count as u32);
            }

            // Render liquid glass primitives with the glass pipeline
            if liquid_count > 0 {
                render_pass.set_pipeline(self.effect_pipelines.glass.as_ref().unwrap());
                render_pass.set_bind_group(0, glass_bind_group, &[]);
                render_pass.draw(
                    0..6,
                    simple_count as u32..(simple_count + liquid_count) as u32,
                );
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render primitives to a backdrop texture for glass blur sampling
    ///
    /// This renders the background primitives to a lower-resolution texture
    /// that glass primitives sample from for their blur effect.
    pub fn render_to_backdrop(
        &mut self,
        backdrop: &wgpu::TextureView,
        _backdrop_size: (u32, u32),
        batch: &PrimitiveBatch,
        has_backdrop_content: bool,
    ) {
        let primitive_background = batch.layer_commands.is_empty()
            && batch.dynamic_images.is_empty()
            && batch.paths.indices.is_empty();
        if batch.has_ordered_draws() && !primitive_background {
            self.render_image_backdrop(backdrop, batch, has_backdrop_content);
            return;
        }
        if batch.primitives.is_empty() {
            if !has_backdrop_content {
                self.clear_target(backdrop, wgpu::Color::BLACK);
            }
            return;
        }

        // Use full viewport size for coordinate mapping, even though texture is smaller.
        self.update_aux_data_buffer(batch);
        // GPU automatically maps NDC space to the texture size, ensuring primitives
        // appear at correct relative positions for glass sampling.
        let main_uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue.write_buffer(
            &self.buffers.uniforms,
            0,
            bytemuck::bytes_of(&main_uniforms),
        );

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(&batch.primitives);

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Backdrop Render Encoder"),
            });

        // Render to backdrop texture
        {
            let backdrop_load = if has_backdrop_content {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(wgpu::Color::BLACK)
            };
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Backdrop Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: backdrop,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: backdrop_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            Self::draw_split_sdf(
                &mut render_pass,
                &self.pipelines,
                &sdf_ranges,
                false,
                self.sdf_vb_buffer(),
            );
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
        // Note: No need to restore uniforms since we're already using main_uniforms
    }

    /// Render glass frame with backdrop and glass primitives in a single encoder submission.
    /// This is more efficient than separate render calls as it reduces command buffer overhead.
    ///
    /// Performs:
    /// 1. Render background primitives to backdrop texture
    /// 2. Render background primitives to target
    /// 3. Render glass primitives with backdrop blur to target
    pub fn render_glass_frame(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        _backdrop_size: (u32, u32), // Not used - we render with full viewport coords
        batch: &PrimitiveBatch,
        has_backdrop_content: bool,
    ) {
        if batch.has_ordered_draws() || batch.has_grouped_glass() {
            self.render_image_glass_frame(
                target,
                backdrop,
                _backdrop_size,
                batch,
                has_backdrop_content,
            );
            return;
        }
        self.ensure_glass_pipelines();

        // Update uniforms for rendering (always use full viewport size)
        // The GPU maps NDC space to actual texture size automatically
        let main_uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };

        // Update auxiliary data buffer
        self.update_aux_data_buffer(batch);

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(&batch.primitives);

        // Split glass primitives into simple and liquid for separate rendering
        let mut simple_primitives: Vec<GpuGlassPrimitive> = Vec::new();
        let mut liquid_primitives: Vec<GpuGlassPrimitive> = Vec::new();
        for prim in &batch.glass_primitives {
            if prim.type_info[0] == GlassType::Simple as u32 {
                simple_primitives.push(*prim);
            } else {
                liquid_primitives.push(*prim);
            }
        }
        let simple_count = simple_primitives.len();
        let liquid_count = liquid_primitives.len();

        // Combine: simple first, then liquid
        let mut ordered_glass_primitives = simple_primitives;
        ordered_glass_primitives.extend(liquid_primitives);

        self.prepare_glass_draw(backdrop, &ordered_glass_primitives);

        // Create single command encoder for entire frame
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Glass Frame Encoder"),
            });

        // Pass 1: Render background primitives to backdrop texture (at half resolution)
        // NOTE: We use main_uniforms (full viewport size) for coordinate mapping,
        // even though the texture is half resolution. The GPU automatically maps
        // NDC space to the texture size. This ensures primitives appear at correct
        // relative positions for glass sampling.
        {
            self.queue.write_buffer(
                &self.buffers.uniforms,
                0,
                bytemuck::bytes_of(&main_uniforms),
            );

            let backdrop_load = if has_backdrop_content {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
            };
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Backdrop Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: backdrop,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: backdrop_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if !batch.primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    false,
                    self.sdf_vb_buffer(),
                );
            }
        }

        // Pass 2: Render background primitives to target (at full resolution)
        {
            self.queue.write_buffer(
                &self.buffers.uniforms,
                0,
                bytemuck::bytes_of(&main_uniforms),
            );

            let target_load = if has_backdrop_content {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(wgpu::Color::BLACK)
            };
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Target Background Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: target_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if !batch.primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    false,
                    self.sdf_vb_buffer(),
                );
            }
        }

        // Pass 3: Render glass primitives with backdrop blur
        if simple_count > 0 || liquid_count > 0 {
            let glass_bind_group = self
                .cached_glass
                .as_ref()
                .unwrap()
                .bind_group
                .as_ref()
                .unwrap();

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Glass Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render simple glass primitives with simple_glass pipeline
            if simple_count > 0 {
                render_pass.set_pipeline(self.effect_pipelines.simple_glass.as_ref().unwrap());
                render_pass.set_bind_group(0, glass_bind_group, &[]);
                render_pass.draw(0..6, 0..simple_count as u32);
            }

            // Render liquid glass primitives with glass pipeline
            if liquid_count > 0 {
                render_pass.set_pipeline(self.effect_pipelines.glass.as_ref().unwrap());
                render_pass.set_bind_group(0, glass_bind_group, &[]);
                render_pass.draw(
                    0..6,
                    simple_count as u32..(simple_count + liquid_count) as u32,
                );
            }
        }

        // Submit background and glass passes first
        self.queue.submit(std::iter::once(encoder.finish()));

        // Pass 3b: Render nested glass primitives (glass inside glass)
        // These are glass elements that are children of other glass elements.
        // They render after parent glass, sampling from the same backdrop.
        if !batch.nested_glass_primitives.is_empty() {
            // Split nested glass into simple and liquid
            let mut nested_simple: Vec<GpuGlassPrimitive> = Vec::new();
            let mut nested_liquid: Vec<GpuGlassPrimitive> = Vec::new();
            for prim in &batch.nested_glass_primitives {
                if prim.type_info[0] == GlassType::Simple as u32 {
                    nested_simple.push(*prim);
                } else {
                    nested_liquid.push(*prim);
                }
            }
            let nested_simple_count = nested_simple.len();
            let nested_liquid_count = nested_liquid.len();

            // Combine: simple first, then liquid
            let mut ordered_nested = nested_simple;
            ordered_nested.extend(nested_liquid);

            self.prepare_glass_draw(backdrop, &ordered_nested);

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Blinc Nested Glass Encoder"),
                });

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Nested Glass Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let nested_bind_group = self
                .cached_glass
                .as_ref()
                .unwrap()
                .bind_group
                .as_ref()
                .unwrap();

            if nested_simple_count > 0 {
                render_pass.set_pipeline(self.effect_pipelines.simple_glass.as_ref().unwrap());
                render_pass.set_bind_group(0, nested_bind_group, &[]);
                render_pass.draw(0..6, 0..nested_simple_count as u32);
            }

            if nested_liquid_count > 0 {
                render_pass.set_pipeline(self.effect_pipelines.glass.as_ref().unwrap());
                render_pass.set_bind_group(0, nested_bind_group, &[]);
                render_pass.draw(
                    0..6,
                    nested_simple_count as u32..(nested_simple_count + nested_liquid_count) as u32,
                );
            }

            drop(render_pass);
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // Foreground groups compose in their own stream after glass.
        self.render_foreground(target, batch);

        // Pass 5: Render paths (SVGs) on top of glass
        // Paths are tessellated geometry that need their own pipeline
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            // Update path buffers (creates/resizes as needed)
            self.update_path_buffers(batch);

            // Render paths
            if let (Some(vb), Some(ib)) = (&self.buffers.path_vertices, &self.buffers.path_indices)
            {
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Blinc Glass Path Encoder"),
                        });

                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Glass Path Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                // Use overlay path pipeline (1x sampled, no MSAA)
                render_pass.set_pipeline(&self.pipelines.path_overlay);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);

                drop(render_pass);
                self.queue.submit(std::iter::once(encoder.finish()));
            }
        }
    }

    /// Lazily create both glass pipelines (liquid glass + simple frosted glass)
    pub(super) fn ensure_glass_pipelines(&mut self) {
        if self.effect_pipelines.glass.is_some() {
            return;
        }

        let glass_source = if self.has_storage_buffers {
            GLASS_SHADER
        } else {
            GLASS_DT_SHADER
        };
        let simple_glass_source = if self.has_storage_buffers {
            SIMPLE_GLASS_SHADER
        } else {
            SIMPLE_GLASS_DT_SHADER
        };

        let glass_shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Glass Shader"),
                source: wgpu::ShaderSource::Wgsl(glass_source.into()),
            });

        let simple_glass_shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Simple Glass Shader"),
                source: wgpu::ShaderSource::Wgsl(simple_glass_source.into()),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Glass Pipeline Layout"),
                bind_group_layouts: &[&self.bind_group_layouts.glass],
                push_constant_ranges: &[],
            });

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

        let color_targets = &[Some(wgpu::ColorTargetState {
            format: self.texture_format,
            blend: Some(blend_state),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        self.effect_pipelines.glass = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Glass Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &glass_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &glass_shader,
                    entry_point: Some("fs_main"),
                    targets: color_targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));

        self.effect_pipelines.simple_glass = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Simple Glass Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &simple_glass_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &simple_glass_shader,
                    entry_point: Some("fs_main"),
                    targets: color_targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));
    }
}
