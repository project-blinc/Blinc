//! Layer targets: pooled textures, compositing layers, and rendering primitive ranges into them.

use super::{GpuRenderer, LayerTexture, LayerTextureCache};
use crate::path::PathVertex;
use crate::primitives::{GpuPrimitive, PathUniforms, PrimitiveBatch, Uniforms};
use crate::shaders::LAYER_COMPOSITE_SHADER;
use wgpu::util::DeviceExt;

impl GpuRenderer {
    /// Get a reference to the layer texture cache
    pub fn layer_texture_cache(&self) -> &LayerTextureCache {
        &self.layer_texture_cache
    }

    /// Get a mutable reference to the layer texture cache
    pub fn layer_texture_cache_mut(&mut self) -> &mut LayerTextureCache {
        &mut self.layer_texture_cache
    }

    /// Acquire a layer texture from the cache
    ///
    /// If a matching texture exists in the pool, it will be reused.
    /// Otherwise, a new texture will be created.
    pub fn acquire_layer_texture(&mut self, size: (u32, u32), with_depth: bool) -> LayerTexture {
        self.layer_texture_cache
            .acquire(&self.device, size, with_depth)
    }

    /// Release a layer texture back to the cache pool
    pub fn release_layer_texture(&mut self, texture: LayerTexture) {
        self.layer_texture_cache.release(texture);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Layer Composition
    // ─────────────────────────────────────────────────────────────────────────────

    /// Create a bind group for layer composition
    pub(super) fn create_layer_composite_bind_group(
        &self,
        uniform_buffer: &wgpu::Buffer,
        layer_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        self.create_layer_composite_bind_group_with_dest(
            uniform_buffer,
            layer_view,
            sampler,
            &self.dummy_blend_dest_view,
            sampler,
        )
    }

    pub(super) fn create_layer_composite_bind_group_with_dest(
        &self,
        uniform_buffer: &wgpu::Buffer,
        layer_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        dest_view: &wgpu::TextureView,
        dest_sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Layer Composite Bind Group"),
            layout: &self.bind_group_layouts.layer_composite,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(layer_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(dest_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(dest_sampler),
                },
            ],
        })
    }

    /// Composite a layer texture onto a target
    ///
    /// Uses the LAYER_COMPOSITE_SHADER to blend the layer onto the target
    /// with the specified blend mode and opacity.
    #[allow(clippy::too_many_arguments)]
    pub fn composite_layer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        layer: &LayerTexture,
        dest_x: f32,
        dest_y: f32,
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) {
        // Create uniform buffer for this composition
        let uniforms = crate::primitives::LayerCompositeUniforms::new(
            layer.size,
            dest_x,
            dest_y,
            (self.viewport_size.0 as f32, self.viewport_size.1 as f32),
            opacity,
            blend_mode,
        );

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Layer Composite Uniforms"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        // Create sampler
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Layer Composite Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create bind group
        let bind_group =
            self.create_layer_composite_bind_group(&uniform_buffer, &layer.view, &sampler);

        // Create render pass and draw
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Layer Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, // Preserve existing content
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&self.pipelines.layer_composite);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..6, 0..1); // 6 vertices for quad (2 triangles)
    }

    /// Composite a layer with source/dest rectangle mapping
    ///
    /// Allows sampling a sub-region of the layer texture and placing it
    /// at a specific destination in the target.
    #[allow(clippy::too_many_arguments)]
    pub fn composite_layer_region(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        layer: &LayerTexture,
        source_rect: blinc_core::Rect,
        dest_rect: blinc_core::Rect,
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) {
        // Convert source rect to normalized UV coordinates
        let layer_w = layer.size.0 as f32;
        let layer_h = layer.size.1 as f32;
        let source_uv = [
            source_rect.x() / layer_w,
            source_rect.y() / layer_h,
            source_rect.width() / layer_w,
            source_rect.height() / layer_h,
        ];

        let uniforms = crate::primitives::LayerCompositeUniforms::with_source_rect(
            source_uv,
            [
                dest_rect.x(),
                dest_rect.y(),
                dest_rect.width(),
                dest_rect.height(),
            ],
            (self.viewport_size.0 as f32, self.viewport_size.1 as f32),
            opacity,
            blend_mode,
        );

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Layer Composite Uniforms"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Layer Composite Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group =
            self.create_layer_composite_bind_group(&uniform_buffer, &layer.view, &sampler);

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Layer Composite Pass"),
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

        render_pass.set_pipeline(&self.pipelines.layer_composite);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..6, 0..1);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Effect Application Methods
    // ─────────────────────────────────────────────────────────────────────────────

    /// Composite two textures together
    ///
    /// Blends `top` over `bottom` using the specified blend mode and opacity.
    pub fn composite_textures(
        &mut self,
        bottom: &wgpu::TextureView,
        top: &wgpu::TextureView,
        output: &wgpu::TextureView,
        size: (u32, u32),
        blend_mode: blinc_core::BlendMode,
        opacity: f32,
    ) {
        use crate::primitives::CompositeUniforms;

        let uniforms = CompositeUniforms {
            opacity,
            blend_mode: blend_mode as u32,
            _padding: [0.0; 2],
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Composite Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composite Bind Group"),
            layout: &self.bind_group_layouts.composite,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(bottom),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(top),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Composite Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Composite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.composite);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a range of primitives to a target
    pub(super) fn render_primitive_range(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        start_idx: usize,
        end_idx: usize,
        clear_color: [f64; 4],
    ) {
        if start_idx >= end_idx {
            return;
        }

        // Extract the primitive range
        let _primitive_count = end_idx - start_idx;
        let primitives = &batch.primitives[start_idx..end_idx];

        if primitives.is_empty() {
            return;
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Sort and upload primitive range
        let sdf_ranges = self.upload_sorted_primitives(primitives);

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Primitive Range Render Encoder"),
            });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Primitive Range Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0],
                            g: clear_color[1],
                            b: clear_color[2],
                            a: clear_color[3],
                        }),
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

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a range of primitives to a tight-fit texture with offset
    ///
    /// This method renders primitives to a texture sized to fit the content,
    /// offsetting primitive positions so they start at (0,0).
    ///
    /// Returns the texture AND the actual content size (which may differ from
    /// texture.size due to pool bucket rounding).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_primitive_range_tight(
        &mut self,
        batch: &PrimitiveBatch,
        start_idx: usize,
        end_idx: usize,
        layer_pos: (f32, f32),
        layer_size: (f32, f32),
        effect_expansion: (f32, f32, f32, f32), // (left, top, right, bottom)
    ) -> (LayerTexture, (u32, u32)) {
        // Forward to the path-aware variant with an empty path range.
        self.render_layer_range_tight(
            batch,
            start_idx,
            end_idx,
            0,
            0,
            0,
            0,
            layer_pos,
            layer_size,
            effect_expansion,
        )
    }

    /// Path-aware variant of `render_primitive_range_tight` — also
    /// renders a slice of `batch.paths` (vertices `[pv_start..pv_end)`,
    /// indices `[pi_start..pi_end)`) into the tight texture, with
    /// vertex positions offset to the layer-local origin. Lottie
    /// shapes go through the path pipeline (lyon-tessellated
    /// triangles), so a layer that contains only paths needs this
    /// variant — the primitive-only one would render an empty
    /// offscreen even though the paths exist in the batch.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_layer_range_tight(
        &mut self,
        batch: &PrimitiveBatch,
        start_idx: usize,
        end_idx: usize,
        pv_start: usize,
        pv_end: usize,
        pi_start: usize,
        pi_end: usize,
        layer_pos: (f32, f32),
        layer_size: (f32, f32),
        effect_expansion: (f32, f32, f32, f32),
    ) -> (LayerTexture, (u32, u32)) {
        // Calculate tight texture size including effect expansion
        let texture_width = (layer_size.0 + effect_expansion.0 + effect_expansion.2)
            .ceil()
            .max(1.0) as u32;
        let texture_height = (layer_size.1 + effect_expansion.1 + effect_expansion.3)
            .ceil()
            .max(1.0) as u32;

        // Round up to reasonable sizes for cache efficiency (64px increments)
        let texture_width = (texture_width.div_ceil(64) * 64).min(self.viewport_size.0);
        let texture_height = (texture_height.div_ceil(64) * 64).min(self.viewport_size.1);

        // Use the allocated extent for raster coordinates and subsequent source UVs.
        let layer_texture =
            self.layer_texture_cache
                .acquire(&self.device, (texture_width, texture_height), false);
        let content_size = layer_texture.size;

        let primitives = if start_idx < end_idx {
            &batch.primitives[start_idx..end_idx]
        } else {
            &[][..]
        };
        let path_verts = if pv_end > pv_start && pv_end <= batch.paths.vertices.len() {
            &batch.paths.vertices[pv_start..pv_end]
        } else {
            &[][..]
        };
        let path_indices = if pi_end > pi_start && pi_end <= batch.paths.indices.len() {
            &batch.paths.indices[pi_start..pi_end]
        } else {
            &[][..]
        };

        if primitives.is_empty() && path_verts.is_empty() {
            return (layer_texture, content_size);
        }

        // Offset primitives so content starts at (effect_expansion.left, effect_expansion.top)
        // This leaves room for effects on the left/top edges
        let offset_x = layer_pos.0 - effect_expansion.0;
        let offset_y = layer_pos.1 - effect_expansion.1;

        let offset_primitives: Vec<GpuPrimitive> = primitives
            .iter()
            .map(|p| {
                let mut op = *p;
                op.bounds[0] -= offset_x;
                op.bounds[1] -= offset_y;
                // Also offset clip bounds if they're valid (not the "no clip" default)
                let has_real_clip = op.clip_bounds[0] > -5000.0 && op.clip_bounds[2] < 90000.0;
                if has_real_clip {
                    op.clip_bounds[0] -= offset_x;
                    op.clip_bounds[1] -= offset_y;
                }
                op
            })
            .collect();

        // Mesh primitives (`type_info[0] == 9`) read their triangle
        // vertex positions from `aux_data[aux_offset..aux_offset + 2]`,
        // not from `bounds`. Those positions are screen-space at push
        // time, so the `bounds` offset above doesn't translate them
        // into the tight texture's coordinate frame — vertices land
        // outside the texture viewport and the rasteriser clips the
        // triangle, leaving the tight texture empty. Build a translated
        // copy of `aux_data` for this pass: clone the original, then
        // subtract the offset from the position vec4s of every mesh
        // primitive in the layer range. The optional per-vertex colour
        // entries that follow (when `type_info[3] == 1`) stay unchanged.
        let mut tight_aux_data: Vec<[f32; 4]> = batch.aux_data.clone();
        let mut needs_aux_upload = super::overlay::translate_clip_frames(
            &mut tight_aux_data,
            offset_primitives
                .iter()
                .map(|p| p.clip_shape[0] as u32)
                .chain(path_verts.iter().map(|v| v.clip_shape)),
            [offset_x, offset_y],
        );
        for op in &offset_primitives {
            if op.type_info[0] != 9 {
                continue;
            }
            let aux_off = op.border[2] as usize;
            if aux_off + 1 >= tight_aux_data.len() {
                continue;
            }
            tight_aux_data[aux_off][0] -= offset_x;
            tight_aux_data[aux_off][1] -= offset_y;
            tight_aux_data[aux_off][2] -= offset_x;
            tight_aux_data[aux_off][3] -= offset_y;
            tight_aux_data[aux_off + 1][0] -= offset_x;
            tight_aux_data[aux_off + 1][1] -= offset_y;
            // pack1.zw is unused (padding) — leave alone.
            needs_aux_upload = true;
        }

        // Polygon-clip vertices in `aux_data` are NOT offset here.
        // `transform_clip_shape` (in paint.rs) stores polygon
        // vertices in ELEMENT-LOCAL coords (DPI-scaled, but no
        // rotation / translation applied), specifically so the
        // fragment shader's `sp - prim.bounds.xy` test ends up in
        // local space — the same space the vertices are in. When we
        // shift `prim.bounds.xy` to (0, 0) for the tight render, the
        // shader's `sp - 0` is already local-space, and the vertices
        // need no shift. Mesh-primitive aux_data above DOES need
        // shifting because mesh vertices are stored in screen-space.
        //
        // HOWEVER we still need to FLAG that aux_data must be
        // re-uploaded — without this, the tight render's polygon test
        // reads from a stale aux_data buffer that the previous main
        // pass populated with BG-batch data (which doesn't contain
        // the scratch batch's polygon vertices). The shader would
        // index aux_data at the scratch's `aux_offset` and find
        // garbage from BG, rejecting all fragments → empty texture.
        // The cn::spinner's arc hit this: the bordered-circle stroke
        // has a polygon clip in its `clip_radius`, but the bake
        // produced an invisible texture until the upload fired.
        for op in &offset_primitives {
            if op.type_info[2] == crate::primitives::ClipType::Polygon as u32 {
                needs_aux_upload = true;
                break;
            }
        }

        // Build the offset PathVertex slice + rebased index buffer.
        // Indices in the source batch reference vertices in
        // `batch.paths.vertices` directly; after slicing, the local
        // vertex array starts at 0, so each index needs `pv_start`
        // subtracted to point at the right vertex inside the slice.
        let offset_path_vertices: Vec<crate::path::PathVertex> = path_verts
            .iter()
            .map(|v| {
                let mut nv = *v;
                nv.position[0] -= offset_x;
                nv.position[1] -= offset_y;
                let has_real_clip = nv.clip_bounds[0] > -5000.0 && nv.clip_bounds[2] < 90000.0;
                if has_real_clip {
                    nv.clip_bounds[0] -= offset_x;
                    nv.clip_bounds[1] -= offset_y;
                }
                nv
            })
            .collect();
        let offset_path_indices: Vec<u32> = path_indices
            .iter()
            .map(|&i| i.saturating_sub(pv_start as u32))
            .collect();

        // Update uniforms with content size (the viewport for this tight render)
        let uniforms = Uniforms {
            viewport_size: [content_size.0 as f32, content_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Sort and upload offset primitives
        let sdf_ranges = self.upload_sorted_primitives(&offset_primitives);
        drop(offset_primitives);

        // Upload the offset-translated `aux_data` if any mesh
        // primitive needed translation. Same `self.buffers.aux_data`
        // the main pass uses — we restore it after `queue.submit`
        // so subsequent passes see the original screen-space data.
        //
        // Use the resize-aware slice path rather than a raw
        // `write_buffer`: the comment used to claim "the buffer is
        // already sized for batch.aux_data's length" but that's only
        // true when the immediately-preceding main pass wrote at
        // least as many entries as we're about to write. The bake-
        // texture path (P4.3 composite-layer scratch batches) often
        // carries polygon-clip vertices or mesh aux that the main
        // pass's bg-batch didn't, so the shared aux buffer is sized
        // for the smaller bg vec (e.g., 16 bytes / 1 vec4) while we
        // need to write 32+ bytes — wgpu errors with "Copy of 0..N
        // would end up overrunning the bounds of the Destination
        // buffer of size M" (cn::spinner bake hit this).
        if needs_aux_upload {
            self.update_aux_data_slice(&tight_aux_data);
        }

        // Upload offset path geometry to a transient buffer pair so
        // the shared `path_vertices` / `path_indices` buffers used by
        // the main pass aren't clobbered.
        use wgpu::util::DeviceExt;
        let path_vb = (!offset_path_vertices.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Tight Path VB"),
                    contents: bytemuck::cast_slice(&offset_path_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });
        let path_ib = (!offset_path_indices.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Tight Path IB"),
                    contents: bytemuck::cast_slice(&offset_path_indices),
                    usage: wgpu::BufferUsages::INDEX,
                })
        });
        let path_index_count = offset_path_indices.len() as u32;
        drop(offset_path_vertices);
        drop(offset_path_indices);

        // Path uniforms: full opacity (the blit applies the layer's
        // opacity), no clip, standard fill. CRITICAL — the path
        // shader reads `uniforms.viewport_size` to convert the
        // vertex position into NDC. The SDF-path `self.buffers.uniforms`
        // we just wrote is a DIFFERENT binding; paths see
        // `self.buffers.path_uniforms`. If we left it at the
        // Default (`[800, 600]`) while rendering into a, say,
        // 192×192 tight texture, the path would end up NDC-scaled
        // by the wrong viewport and land off-centre at massive
        // scale (visible as an oversized, drifted glass body).
        let path_uniforms = crate::primitives::PathUniforms {
            viewport_size: [content_size.0 as f32, content_size.1 as f32],
            ..crate::primitives::PathUniforms::default()
        };
        self.queue.write_buffer(
            &self.buffers.path_uniforms,
            0,
            bytemuck::bytes_of(&path_uniforms),
        );

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tight Render Encoder"),
            });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tight Primitive Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &layer_texture.view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if !primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    false,
                    self.sdf_vb_buffer(),
                );
            }

            if let (Some(vb), Some(ib)) = (&path_vb, &path_ib) {
                render_pass.set_pipeline(&self.pipelines.path);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..path_index_count, 0, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Restore viewport uniforms for subsequent operations
        let restore_uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue.write_buffer(
            &self.buffers.uniforms,
            0,
            bytemuck::bytes_of(&restore_uniforms),
        );

        // Restore the screen-space `aux_data` so subsequent passes
        // (next layer's tight render, post-effect overlays, etc.)
        // see the original mesh / polygon vertex positions instead of
        // the tight-translated copy we wrote above. Resize-aware path
        // for the same reason as the upload above.
        if needs_aux_upload {
            self.update_aux_data_slice(&batch.aux_data);
        }

        (layer_texture, content_size)
    }

    /// Bake a scratch batch, including nested groups and paths, into an exact
    /// tight texture. Positions are screen-space; `layer_pos` becomes its origin.
    /// The returned texture contains straight alpha for the per-frame blit.
    pub fn render_subtree_to_layer_texture(
        &mut self,
        batch: &PrimitiveBatch,
        layer_pos: (f32, f32),
        layer_size: (f32, f32),
    ) -> (LayerTexture, (u32, u32)) {
        self.render_composite_subtree(batch, layer_pos, layer_size)
    }

    /// Override viewport size for offscreen rendering to a smaller texture.
    /// This swaps `self.viewport_size` so all render functions (text, images, SDF)
    /// use the offscreen size for NDC conversion. Must call `restore_viewport()` after.
    pub fn set_viewport_override(&mut self, size: (u32, u32)) {
        self.saved_viewport_size = Some(self.viewport_size);
        self.viewport_size = size;
    }

    /// Restore viewport size after offscreen rendering.
    pub fn restore_viewport(&mut self) {
        if let Some(saved) = self.saved_viewport_size.take() {
            self.viewport_size = saved;
        }
    }
}
