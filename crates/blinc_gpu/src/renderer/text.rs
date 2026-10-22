//! Standalone prepared text consumes the auxiliary rows of its owning batch.
use super::{GpuGlyph, GpuRenderer, PrimitiveBatch, Uniforms};

impl GpuRenderer {
    /// Render the separate glyph stream with its owning path clip rows.
    /// The batch's shapes, paths and glass are not dispatched by this method.
    pub fn render_text_batch(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        atlas_view: &wgpu::TextureView,
        color_atlas_view: &wgpu::TextureView,
        atlas_sampler: &wgpu::Sampler,
    ) {
        if batch.glyphs.is_empty() {
            return;
        }
        self.update_aux_data_for_batch(batch);
        self.render_text(
            target,
            &batch.glyphs,
            atlas_view,
            color_atlas_view,
            atlas_sampler,
        );
    }

    /// Render text glyphs with a provided atlas texture
    ///
    /// # Arguments
    /// * `target` - The texture view to render to
    /// * `glyphs` - The glyph instances to render
    /// * `atlas_view` - The grayscale glyph atlas texture view
    /// * `color_atlas_view` - The color (RGBA) glyph atlas texture view for emoji
    /// * `atlas_sampler` - The sampler for the atlases
    ///
    /// For glyphs carrying path headers, prefer `Self::render_text_batch`.
    /// Low-level callers must upload the matching auxiliary rows first.
    pub fn render_text(
        &mut self,
        target: &wgpu::TextureView,
        glyphs: &[GpuGlyph],
        atlas_view: &wgpu::TextureView,
        color_atlas_view: &wgpu::TextureView,
        atlas_sampler: &wgpu::Sampler,
    ) {
        if glyphs.is_empty() {
            return;
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update glyphs: storage buffer or data texture
        if self.has_storage_buffers {
            self.queue
                .write_buffer(&self.buffers.glyphs, 0, bytemuck::cast_slice(glyphs));
        } else if let Some(ref tex) = self.buffers.glyph_data_texture {
            if !glyphs.is_empty() {
                let bytes = bytemuck::cast_slice(glyphs);
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    bytes,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(6 * 16), // 6 texels × 16 bytes per Rgba32Float = 96 bytes = sizeof(GpuGlyph)
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width: 6,
                        height: glyphs.len() as u32,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        // Atlas views, samplers and auxiliary allocations can change between draws.
        let text_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Bind Group"),
            layout: &self.bind_group_layouts.text,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.buffers.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: if self.has_storage_buffers {
                        self.buffers.glyphs.as_entire_binding()
                    } else {
                        wgpu::BindingResource::TextureView(
                            self.buffers.glyph_data_view.as_ref().unwrap(),
                        )
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(atlas_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(color_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: if self.has_storage_buffers {
                        self.buffers.aux_data.as_entire_binding()
                    } else {
                        wgpu::BindingResource::TextureView(
                            self.buffers.aux_data_view.as_ref().unwrap(),
                        )
                    },
                },
            ],
        });
        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Text Render Encoder"),
            });

        // Begin render pass (load existing content)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Text Render Pass"),
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

            // Use text_overlay pipeline since we're rendering to 1x sampled texture
            render_pass.set_pipeline(&self.pipelines.text_overlay);
            render_pass.set_bind_group(0, &text_bind_group, &[]);
            // Apply staged scissor (compositor v2 damage-rect path)
            // so this dispatch only paints into the same rect that
            // the SDF clear + redraw covered. Without this guard a
            // text glyph emitted into a motion-bound subtree would
            // paint outside the damaged region and stick on top of
            // the static cache's previous-frame pixels.
            if let Some((sx, sy, sw, sh)) = self.pending_scissor {
                render_pass.set_scissor_rect(sx, sy, sw, sh);
            }
            render_pass.draw(0..6, 0..glyphs.len() as u32);
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
