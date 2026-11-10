//! The multisampled overlay passes.

use super::GpuRenderer;
use crate::primitives::{GpuPrimitive, PrimitiveBatch, Uniforms};

impl GpuRenderer {
    /// Render overlay primitives with MSAA anti-aliasing
    ///
    /// This method renders paths/primitives to a temporary MSAA texture,
    /// resolves it, and then blends onto the target. This provides smooth
    /// edges for tessellated paths that don't have shader-based AA.
    ///
    /// # Arguments
    /// * `target` - The single-sampled texture view to render to (existing content is preserved)
    /// * `batch` - The primitive batch to render
    /// * `sample_count` - MSAA sample count (typically 4)
    ///
    /// Layer push/pop commands retain paint order and apply group effects.
    /// Non-normal layer blend modes require `COPY_SRC` on the target texture.
    pub fn render_overlay_msaa(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        sample_count: u32,
    ) {
        if batch.has_layer_commands() || batch.has_ordered_draws() {
            self.render_layered_overlay_msaa(target, batch, sample_count);
        } else {
            self.render_overlay_msaa_flat(target, batch, sample_count);
        }
    }

    pub(super) fn render_overlay_msaa_flat(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        sample_count: u32,
    ) {
        if batch.paths.vertices.is_empty() && batch.primitives.is_empty() {
            return;
        }

        self.ensure_overlay_alpha();

        // Ensure we have MSAA pipelines for this sample count
        let need_new_pipelines = match &self.msaa_pipelines {
            Some(p) => p.sample_count != sample_count,
            None => true,
        };
        if need_new_pipelines && sample_count > 1 {
            self.msaa_pipelines = Some(Self::create_msaa_pipelines(
                &self.device,
                &self.bind_group_layouts,
                self.texture_format,
                sample_count,
                self.has_vertex_storage,
                self.has_storage_buffers,
            ));
        }

        let (width, height) = self.viewport_size;

        let overlay = self.overlay_target(sample_count);

        // Update uniforms. `_padding.x = 1.0` signals the SDF fragment
        // shader that hardware multisample coverage is driving
        // silhouette AA for this pass — see `render_primitives_overlay_msaa`
        // for the longer rationale on why stacking the shader's
        // per-edge fade on top of MSAA resolve under-fills every
        // partially-covered pixel. The flag is only honoured by the
        // mesh-primitive branch inside `sdf_core`; every other branch
        // ignores `_padding`.
        let msaa_flag = if sample_count > 1 { 1.0 } else { 0.0 };
        let uniforms = Uniforms {
            viewport_size: [width as f32, height as f32],
            _padding: [msaa_flag, 0.0],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(&batch.primitives);

        // Upload auxiliary data buffer so PRIM_MESH triangle corners and
        // polygon-clip vertices resolve to live GPU data. The
        // single-sampled overlay path piggybacks on the upload that
        // `render_with_clear_simple` does at the top of the frame, but
        // callers that enter this MSAA path *in place of*
        // `render_with_clear` skip that upload and the GPU buffer then
        // carries stale data from the previous frame — every mesh
        // triangle indexes into random `aux_data` and either renders as
        // nothing or as a degenerate sliver. Updating here keeps the
        // MSAA overlay self-sufficient.
        self.update_aux_data_buffer(batch);

        // Update path buffers
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Get references to the cached textures (after mutable borrows are done)
        let cached = self.overlay_targets.get(overlay);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Overlay MSAA Render Encoder"),
            });

        // Pass 1: Render paths + SDF primitives to an offscreen texture.
        //
        // WebGPU spec: `resolveTarget` MUST be None when the color
        // attachment's sample_count is 1. Chrome/Dawn silently accept
        // a stray `resolve_target: Some(...)` on a single-sampled
        // view, but Safari/WebKit rejects the render pass entirely —
        // which is why every path draw (notches, SVG strokes, custom
        // paths) was invisible on Safari.
        //
        // Fix: when sample_count == 1, render directly into
        // `resolve_view` (a single-sampled texture with both
        // RENDER_ATTACHMENT and TEXTURE_BINDING usage) and pass
        // `resolve_target: None`. When sample_count > 1, use the
        // multisampled `msaa_view` with a resolve into `resolve_view`
        // as before.
        let (pass1_view, pass1_resolve, pass1_store) = if let Some(msaa_view) = &cached.msaa_view {
            // Multisampled: keep the resolved single-sample texture,
            // discard the MSAA texture (we only wanted its resolved
            // output, not the per-sample content).
            (
                msaa_view,
                Some(&cached.resolve_view),
                wgpu::StoreOp::Discard,
            )
        } else {
            // Single-sampled: render directly into the resolve_view
            // and keep the content so pass 2 can sample from it.
            (&cached.resolve_view, None, wgpu::StoreOp::Store)
        };
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay MSAA Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: pass1_view,
                    resolve_target: pass1_resolve,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: pass1_store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Get the appropriate path pipeline for the sample count
            let path_pipeline = if sample_count > 1 {
                if let Some(ref msaa) = self.msaa_pipelines {
                    &msaa.path
                } else {
                    &self.pipelines.path
                }
            } else {
                &self.pipelines.path
            };

            // Render paths using MSAA pipeline
            if has_paths
                && let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
            {
                render_pass.set_pipeline(path_pipeline);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
            }

            // Render SDF primitives using split MSAA pipelines
            if !batch.primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                if sample_count > 1 {
                    if let Some(ref msaa) = self.msaa_pipelines {
                        Self::draw_split_sdf_msaa(
                            &mut render_pass,
                            msaa,
                            &sdf_ranges,
                            self.sdf_vb_buffer(),
                        );
                    } else {
                        Self::draw_split_sdf(
                            &mut render_pass,
                            &self.pipelines,
                            &sdf_ranges,
                            false,
                            self.sdf_vb_buffer(),
                        );
                    }
                } else {
                    Self::draw_split_sdf(
                        &mut render_pass,
                        &self.pipelines,
                        &sdf_ranges,
                        false,
                        self.sdf_vb_buffer(),
                    );
                }
            }
        }

        // Pass 2: Blend resolved texture onto target using cached resources
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay Blend Pass"),
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

            render_pass.set_pipeline(&self.overlay_alpha.as_ref().unwrap().premultiplied);
            render_pass.set_bind_group(0, &cached.composite_bind_group, &[]);
            render_pass.draw(0..3, 0..1); // Fullscreen triangle
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render only paths with MSAA anti-aliasing
    ///
    /// This is used when SDF primitives are rendered separately (unified rendering mode)
    /// but paths still need MSAA for smooth edges.
    pub fn render_paths_overlay_msaa(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        sample_count: u32,
    ) {
        if batch.paths.vertices.is_empty() || batch.paths.indices.is_empty() {
            return;
        }

        // Ensure we have MSAA pipelines for this sample count
        let need_new_pipelines = match &self.msaa_pipelines {
            Some(p) => p.sample_count != sample_count,
            None => true,
        };
        if need_new_pipelines && sample_count > 1 {
            self.msaa_pipelines = Some(Self::create_msaa_pipelines(
                &self.device,
                &self.bind_group_layouts,
                self.texture_format,
                sample_count,
                self.has_vertex_storage,
                self.has_storage_buffers,
            ));
        }

        let (width, height) = self.viewport_size;

        let overlay = self.overlay_target(sample_count);

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [width as f32, height as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        self.update_aux_data_buffer(batch);

        // Update path buffers
        self.update_path_buffers(batch);

        // Get references to the cached textures
        let cached = self.overlay_targets.get(overlay);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Path MSAA Render Encoder"),
            });

        // Pass 1: Render paths to an offscreen texture.
        //
        // See the longer comment in `render_overlay_msaa` — the key
        // constraint is that `resolveTarget` must be None when the
        // color attachment is single-sampled (WebGPU spec). Safari
        // enforces this; Chrome accepts a stray `Some(...)` silently.
        let (pass1_view, pass1_resolve, pass1_store) = if let Some(msaa_view) = &cached.msaa_view {
            (
                msaa_view,
                Some(&cached.resolve_view),
                wgpu::StoreOp::Discard,
            )
        } else {
            (&cached.resolve_view, None, wgpu::StoreOp::Store)
        };
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Path MSAA Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: pass1_view,
                    resolve_target: pass1_resolve,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: pass1_store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Get the appropriate pipeline for the sample count
            let path_pipeline = if sample_count > 1 {
                if let Some(ref msaa) = self.msaa_pipelines {
                    &msaa.path
                } else {
                    &self.pipelines.path
                }
            } else {
                &self.pipelines.path
            };

            if let (Some(vb), Some(ib)) = (&self.buffers.path_vertices, &self.buffers.path_indices)
            {
                render_pass.set_pipeline(path_pipeline);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
            }
        }

        // Pass 2: Blend resolved texture onto target
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Path Blend Pass"),
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

            render_pass.set_pipeline(&self.pipelines.composite_overlay);
            render_pass.set_bind_group(0, &cached.composite_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a slice of SDF primitives with MSAA anti-aliasing.
    ///
    /// Mirrors [`Self::render_primitives_overlay`] but dispatches through
    /// the MSAA-enabled split SDF pipelines so mesh primitives
    /// (`PRIM_MESH` — tessellated path solid fills) and the other SDF
    /// categories get the same hardware-resolved smoothing that paths
    /// and gradients already receive via
    /// [`Self::render_paths_overlay_msaa`].
    ///
    /// Without this, unified-text mode routed solid path fills through
    /// `render_unified` (single-sampled) and only the gradient / stroke
    /// paths took the MSAA overlay, which left vector animation output
    /// visibly rougher than the rasterized-SVG path even though both
    /// tessellate cubic Beziers at the same tolerance.
    ///
    /// Pattern matches `render_paths_overlay_msaa`: render into the
    /// cached MSAA texture (or directly into the single-sampled resolve
    /// view when `sample_count == 1`, required by WebGPU spec so Safari
    /// doesn't reject the pass), then blend the resolved texture onto
    /// `target` via the composite overlay pipeline.
    pub fn render_primitives_overlay_msaa(
        &mut self,
        target: &wgpu::TextureView,
        primitives: &[GpuPrimitive],
        sample_count: u32,
    ) {
        if primitives.is_empty() {
            return;
        }

        // Ensure MSAA pipelines exist at the requested sample count.
        let need_new_pipelines = match &self.msaa_pipelines {
            Some(p) => p.sample_count != sample_count,
            None => true,
        };
        if need_new_pipelines && sample_count > 1 {
            self.msaa_pipelines = Some(Self::create_msaa_pipelines(
                &self.device,
                &self.bind_group_layouts,
                self.texture_format,
                sample_count,
                self.has_vertex_storage,
                self.has_storage_buffers,
            ));
        }

        let (width, height) = self.viewport_size;

        let overlay = self.overlay_target(sample_count);

        // Update viewport uniform for this pass.
        //
        // `_padding.x` carries an MSAA flag: 1.0 here tells the SDF
        // fragment shader that hardware multisample coverage is driving
        // silhouette AA, so the mesh-primitive branch should skip its
        // own per-edge barycentric fade. The fade works well on
        // single-sampled targets (web fallback) but stacks with MSAA
        // resolve to under-fill partially-covered pixels — a pixel with
        // 50 % coverage resolves to ~0.05 alpha instead of 0.5 because
        // each rendered sample already carries a faded-down value
        // before the average. Removing the redundant shader fade
        // restores the resolve to hardware-accurate coverage.
        let msaa_flag = if sample_count > 1 { 1.0 } else { 0.0 };
        let uniforms = Uniforms {
            viewport_size: [width as f32, height as f32],
            _padding: [msaa_flag, 0.0],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Sort + upload primitives into the shared instance / aux buffers
        // exactly the way `render_primitives_overlay` does.
        let sdf_ranges = self.upload_sorted_primitives(primitives);

        let cached = self.overlay_targets.get(overlay);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Primitives MSAA Render Encoder"),
            });

        // Pass 1: draw into MSAA (or single-sampled resolve) texture.
        // WebGPU: `resolve_target` must be None when the color
        // attachment is single-sampled, so branch on `sample_count`.
        let (pass1_view, pass1_resolve, pass1_store) = if let Some(msaa_view) = &cached.msaa_view {
            (
                msaa_view,
                Some(&cached.resolve_view),
                wgpu::StoreOp::Discard,
            )
        } else {
            (&cached.resolve_view, None, wgpu::StoreOp::Store)
        };
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Primitives MSAA Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: pass1_view,
                    resolve_target: pass1_resolve,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: pass1_store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            if sample_count > 1 {
                if let Some(ref msaa) = self.msaa_pipelines {
                    Self::draw_split_sdf_msaa(
                        &mut render_pass,
                        msaa,
                        &sdf_ranges,
                        self.sdf_vb_buffer(),
                    );
                } else {
                    Self::draw_split_sdf(
                        &mut render_pass,
                        &self.pipelines,
                        &sdf_ranges,
                        true,
                        self.sdf_vb_buffer(),
                    );
                }
            } else {
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    true,
                    self.sdf_vb_buffer(),
                );
            }
        }

        // Pass 2: composite the resolved MSAA texture over `target`,
        // preserving existing content (LoadOp::Load).
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Primitives Blend Pass"),
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

            render_pass.set_pipeline(&self.pipelines.composite_overlay);
            render_pass.set_bind_group(0, &cached.composite_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
