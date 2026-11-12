//! Rendering the overlay passes onto a resolved target: primitives, paths and glyphs.

use super::GpuRenderer;
use crate::primitives::{GpuPrimitive, PrimitiveBatch, Uniforms};

impl GpuRenderer {
    /// Render primitives as an overlay on existing content (1x sampled)
    ///
    /// This uses the overlay pipeline which is configured for sample_count=1,
    /// making it suitable for rendering on top of already-resolved content
    /// (e.g., after glass effects have been applied).
    ///
    /// # Arguments
    /// * `target` - The single-sampled texture view to render to (existing content is preserved)
    /// * `batch` - The primitive batch to render
    pub fn render_overlay(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        if batch.has_ordered_draws() {
            self.render_layered_overlay_msaa(target, batch, 1);
            return;
        }
        // Phase 4a: include pure-opacity layers in the gate so an
        // overlay containing only opacity-animated content takes the
        // layer-aware path (which actually composites with the
        // configured opacity) instead of the simple path (which drops
        // layer commands entirely).
        let has_layer_processing = batch.layer_commands.iter().any(|entry| {
            if let crate::primitives::LayerCommand::Push { config } = &entry.command {
                !config.effects.is_empty()
                    || config.blend_mode != blinc_core::BlendMode::Normal
                    || config.opacity < 1.0
            } else {
                false
            }
        });

        // If we have layer effects or blend modes, use the layer-aware rendering path
        if has_layer_processing {
            self.render_overlay_with_layer_effects(target, batch);
            return;
        }

        // Standard overlay rendering (no layer effects)
        // Update uniforms
        let uniforms = Uniforms::new([self.viewport_size.0 as f32, self.viewport_size.1 as f32]);
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update auxiliary data buffer
        self.update_aux_data_buffer(batch);

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(&batch.primitives);

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Overlay Render Encoder"),
            });

        // Begin render pass (load existing content, don't clear)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Overlay Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None, // No MSAA resolve needed for overlay
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

            // Render paths first (they're typically backgrounds)
            if has_paths
                && let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
            {
                render_pass.set_pipeline(&self.pipelines.path_overlay);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
            }

            // Render SDF primitives using split overlay pipelines
            if !batch.primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    true,
                    self.sdf_vb_buffer(),
                );
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render overlay with layer effect/blend-mode processing
    ///
    /// Follows the same pattern as render_with_layer_effects:
    /// 1. Build list of layers that need processing (effects or blend modes)
    /// 2. Render non-layer primitives normally (overlay = LoadOp::Load)
    /// 3. For each layer, render to tight offscreen texture, apply effects, blit at position
    pub(super) fn render_overlay_with_layer_effects(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
    ) {
        use crate::primitives::LayerCommand;

        // Build list of layers with their primitive ranges
        let mut effect_layers: Vec<(usize, usize, blinc_core::LayerConfig)> = Vec::new();
        let mut layer_stack: Vec<(usize, blinc_core::LayerConfig)> = Vec::new();

        for entry in &batch.layer_commands {
            match &entry.command {
                LayerCommand::Push { config } => {
                    layer_stack.push((entry.primitive_index, config.clone()));
                }
                LayerCommand::Pop => {
                    if let Some((start_idx, config)) = layer_stack.pop() {
                        // Phase 4a: pure-opacity layers also need
                        // processing so `config.opacity` reaches the
                        // composite blit instead of being silently
                        // dropped.
                        let needs_processing = !config.effects.is_empty()
                            || config.blend_mode != blinc_core::BlendMode::Normal
                            || config.transform_3d.is_some()
                            || config.opacity < 1.0;
                        if needs_processing {
                            effect_layers.push((start_idx, entry.primitive_index, config));
                        }
                    }
                }
                LayerCommand::Glass(_)
                | LayerCommand::Meshes(_)
                | LayerCommand::GpuPass(_)
                | LayerCommand::Sample { .. }
                | LayerCommand::DeferredText { .. }
                | LayerCommand::DeferredSvg { .. }
                | LayerCommand::DeferredImage { .. } => {}
            }
        }

        if effect_layers.is_empty() {
            self.render_overlay_simple(target, batch);
            return;
        }

        // Build set of primitive indices that belong to effect/blend layers (skip in first pass)
        let mut effect_primitives = std::collections::HashSet::new();
        for (start, end, _) in &effect_layers {
            for i in *start..*end {
                effect_primitives.insert(i);
            }
        }

        // First pass: render primitives NOT in effect/blend layers (overlay = Load)
        self.render_overlay_primitives_excluding(target, batch, &effect_primitives);
        drop(effect_primitives);

        // Process each effect/blend layer
        for (start_idx, end_idx, config) in effect_layers {
            if start_idx >= end_idx || end_idx > batch.primitives.len() {
                continue;
            }

            // Compute bounding box from primitives (screen coordinates)
            let primitives = &batch.primitives[start_idx..end_idx];
            let (layer_pos, layer_size, layer_clip) = if primitives.is_empty() {
                let pos = config.position.map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
                let size = config
                    .size
                    .map(|s| (s.width, s.height))
                    .unwrap_or((self.viewport_size.0 as f32, self.viewport_size.1 as f32));
                (pos, size, None)
            } else {
                let mut min_x = f32::MAX;
                let mut min_y = f32::MAX;
                let mut max_x = f32::MIN;
                let mut max_y = f32::MIN;
                let mut clip: Option<([f32; 4], [f32; 4])> = None;
                for p in primitives {
                    let (px, py, pw, ph) = (p.bounds[0], p.bounds[1], p.bounds[2], p.bounds[3]);
                    min_x = min_x.min(px);
                    min_y = min_y.min(py);
                    max_x = max_x.max(px + pw);
                    max_y = max_y.max(py + ph);
                    if clip.is_none() && p.clip_bounds[0] > -5000.0 && p.clip_bounds[2] < 90000.0 {
                        clip = Some((p.clip_bounds, p.clip_radius));
                    }
                }
                let width = (max_x - min_x).max(1.0);
                let height = (max_y - min_y).max(1.0);
                ((min_x, min_y), (width, height), clip)
            };

            // Skip layers entirely outside the viewport
            let vp_w = self.viewport_size.0 as f32;
            let vp_h = self.viewport_size.1 as f32;
            let is_visible = layer_pos.0 < vp_w
                && layer_pos.1 < vp_h
                && layer_pos.0 + layer_size.0 > 0.0
                && layer_pos.1 + layer_size.1 > 0.0
                && layer_size.0 > 0.0
                && layer_size.1 > 0.0;

            if !is_visible {
                continue;
            }

            let effect_expansion = Self::calculate_effect_expansion(&config.effects);

            // Render layer primitives to tight texture with offset
            let (layer_texture, content_size) = self.render_primitive_range_tight(
                batch,
                start_idx,
                end_idx,
                layer_pos,
                layer_size,
                effect_expansion,
            );

            let tight_size = content_size;
            let expanded_pos = (
                layer_pos.0 - effect_expansion.0,
                layer_pos.1 - effect_expansion.1,
            );
            let expanded_size = (
                layer_size.0 + effect_expansion.0 + effect_expansion.2,
                layer_size.1 + effect_expansion.1 + effect_expansion.3,
            );

            if config.effects.is_empty() {
                // Blend-mode only: blit directly
                self.blit_tight_texture_to_target(
                    &layer_texture.view,
                    tight_size,
                    target,
                    expanded_pos,
                    expanded_size,
                    config.opacity,
                    config.blend_mode,
                    layer_clip,
                    config.transform_3d,
                    0.0,
                );
                self.layer_texture_cache.release(layer_texture);
            } else {
                // Apply effects then blit
                let effected = self.apply_layer_effects(&layer_texture, &config.effects);
                self.layer_texture_cache.release(layer_texture);

                self.blit_tight_texture_to_target(
                    &effected.view,
                    tight_size,
                    target,
                    expanded_pos,
                    expanded_size,
                    config.opacity,
                    config.blend_mode,
                    layer_clip,
                    config.transform_3d,
                    0.0,
                );
                self.layer_texture_cache.release(effected);
            }
        }
    }

    /// Render overlay primitives excluding those in the given set (LoadOp::Load)
    pub(super) fn render_overlay_primitives_excluding(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        exclude: &std::collections::HashSet<usize>,
    ) {
        if exclude.is_empty() {
            self.render_overlay_simple(target, batch);
            return;
        }

        let included_primitives: Vec<GpuPrimitive> = batch
            .primitives
            .iter()
            .enumerate()
            .filter(|(i, _)| !exclude.contains(i))
            .map(|(_, p)| *p)
            .collect();

        // Update uniforms
        let uniforms = Uniforms::new([self.viewport_size.0 as f32, self.viewport_size.1 as f32]);
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update auxiliary data buffer
        self.update_aux_data_buffer(batch);

        // Update path buffers
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Sort and upload filtered primitives
        let sdf_ranges = self.upload_sorted_primitives(&included_primitives);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Overlay Excluding Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Overlay Excluding Pass"),
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

            // Render paths first
            if has_paths
                && let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
            {
                render_pass.set_pipeline(&self.pipelines.path_overlay);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
            }

            // Render filtered SDF primitives via split overlay pipelines
            if !included_primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    true,
                    self.sdf_vb_buffer(),
                );
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Simple overlay render without layer effect processing
    pub(super) fn render_overlay_simple(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
    ) {
        // Update uniforms
        let uniforms = Uniforms::new([self.viewport_size.0 as f32, self.viewport_size.1 as f32]);
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        self.update_aux_data_buffer(batch);

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(&batch.primitives);

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Overlay Simple Render Encoder"),
            });

        // Begin render pass (load existing content, don't clear)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Overlay Simple Render Pass"),
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

            // Render paths first
            if has_paths
                && let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
            {
                render_pass.set_pipeline(&self.pipelines.path_overlay);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
            }

            // Render SDF primitives via split overlay pipelines
            if !batch.primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    true,
                    self.sdf_vb_buffer(),
                );
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a slice of primitives as overlay (LoadOp::Load, keeps existing content)
    ///
    /// This is used for interleaved z-layer rendering where primitives need
    /// to be rendered per-layer to properly interleave with text.
    /// Uses `self.bind_groups.sdf` which automatically includes the real glyph
    /// atlas when `set_glyph_atlas()` was called at the start of the frame.
    pub fn render_primitives_overlay(
        &mut self,
        target: &wgpu::TextureView,
        primitives: &[GpuPrimitive],
    ) {
        if primitives.is_empty() {
            return;
        }

        // Update uniforms
        let uniforms = Uniforms::new([self.viewport_size.0 as f32, self.viewport_size.1 as f32]);
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(primitives);

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Layer Primitives Encoder"),
            });

        // Begin render pass (load existing content)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Layer Primitives Pass"),
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

            if let Some((x, y, w, h)) = self.pending_scissor {
                render_pass.set_scissor_rect(x, y, w, h);
            }
            // Render SDF primitives via split overlay pipelines
            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            Self::draw_split_sdf(
                &mut render_pass,
                &self.pipelines,
                &sdf_ranges,
                true,
                self.sdf_vb_buffer(),
            );
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render paths (tessellated geometry like SVGs) as an overlay
    ///
    /// This renders paths on top of existing content without clearing.
    /// Used for z-layered rendering where paths need to be rendered separately.
    pub fn render_paths_overlay(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if !has_paths {
            return;
        }

        self.update_aux_data_buffer(batch);

        // Update path buffers
        self.update_path_buffers(batch);

        // Render paths
        if let (Some(vb), Some(ib)) = (&self.buffers.path_vertices, &self.buffers.path_indices) {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Blinc Paths Overlay Encoder"),
                });

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Paths Overlay Pass"),
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

            // Use overlay path pipeline (1x sampled)
            render_pass.set_pipeline(&self.pipelines.path_overlay);
            render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
            render_pass.set_vertex_buffer(0, vb.slice(..));
            render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);

            drop(render_pass);
            self.queue.submit(std::iter::once(encoder.finish()));
        }
    }

    /// Render SDF primitives with unified text rendering (text as primitives)
    ///
    /// This method renders SDF primitives including text glyphs in a single pass.
    /// Text primitives (PrimitiveType::Text) sample from the provided glyph atlases.
    /// Uses `set_glyph_atlas()` to bind the real atlas, then delegates to
    /// `render_primitives_overlay()`.
    pub fn render_primitives_overlay_with_glyphs(
        &mut self,
        target: &wgpu::TextureView,
        primitives: &[GpuPrimitive],
        atlas_view: &wgpu::TextureView,
        color_atlas_view: &wgpu::TextureView,
    ) {
        self.set_glyph_atlas(atlas_view, color_atlas_view);
        self.render_primitives_overlay(target, primitives);
    }
}
