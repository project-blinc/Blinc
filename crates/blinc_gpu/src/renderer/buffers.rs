//! The GPU buffers and bind groups, and writing the primitive, path and auxiliary data into them.

use super::{BindGroupLayouts, BindGroups, Buffers, GpuRenderer, RendererConfig};
use crate::gradient_texture::GradientTextureCache;
use crate::path::PathVertex;
use crate::primitives::{
    GlassUniforms, GpuGlassPrimitive, GpuGlyph, GpuPrimitive, PathUniforms, PrimitiveBatch,
    Uniforms,
};

impl GpuRenderer {
    pub(super) fn create_buffers(
        device: &wgpu::Device,
        config: &RendererConfig,
        has_storage_buffers: bool,
    ) -> Buffers {
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniforms Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Storage buffers are always created (even in DT mode they are needed by
        // non-SDF pipelines like glass). In DT mode the SDF bind group uses data
        // textures instead, but these buffers remain for other uses.
        let primitives = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Primitives Buffer"),
            size: (std::mem::size_of::<GpuPrimitive>() * config.max_primitives) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let glass_primitives = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Glass Primitives Buffer"),
            size: (std::mem::size_of::<GpuGlassPrimitive>() * config.max_glass_primitives) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let glass_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Glass Uniforms Buffer"),
            size: std::mem::size_of::<GlassUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let glyphs = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Glyphs Buffer"),
            size: (std::mem::size_of::<GpuGlyph>() * config.max_glyphs) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let path_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Path Uniforms Buffer"),
            size: std::mem::size_of::<PathUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Auxiliary data buffer for variable-length per-primitive data
        // Initial size: 1 vec4 (minimum for valid binding, will be recreated if needed)
        let aux_data = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Aux Data Buffer"),
            size: 16, // 1 vec4<f32> minimum
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create data textures for DT (Tier 3) fallback when no storage buffers
        let (
            prim_data_texture,
            prim_data_view,
            aux_data_texture,
            aux_data_view,
            aux_data_texture_height,
            glyph_data_texture,
            glyph_data_view,
        ) = if !has_storage_buffers {
            // One texel per vec4 field, one complete primitive per row.
            let prim_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Primitive Data Texture"),
                size: wgpu::Extent3d {
                    width: GpuPrimitive::DATA_TEXTURE_TEXELS,
                    height: config.max_primitives as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let prim_view = prim_tex.create_view(&wgpu::TextureViewDescriptor::default());

            // Aux data texture: width=1024, height=1 initially (resized on demand)
            let aux_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Aux Data Texture"),
                size: wgpu::Extent3d {
                    width: 1024,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let aux_view = aux_tex.create_view(&wgpu::TextureViewDescriptor::default());

            // Glyph data texture: width=6 (one texel per vec4 field), height=max_glyphs
            let glyph_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Glyph Data Texture"),
                size: wgpu::Extent3d {
                    width: 6,
                    height: config.max_glyphs as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let glyph_view = glyph_tex.create_view(&wgpu::TextureViewDescriptor::default());

            (
                Some(prim_tex),
                Some(prim_view),
                Some(aux_tex),
                Some(aux_view),
                1u32,
                Some(glyph_tex),
                Some(glyph_view),
            )
        } else {
            (None, None, None, None, 0u32, None, None)
        };

        Buffers {
            uniforms,
            primitives,
            glass_primitives,
            glass_uniforms,
            glyphs,
            path_uniforms,
            path_vertices: None,
            path_indices: None,
            blur_uniforms_pool: None,
            drop_shadow_uniforms: None,
            glow_uniforms: None,
            color_matrix_uniforms: None,
            aux_data,
            sdf_vertex_instances: None,
            prim_data_texture,
            prim_data_view,
            aux_data_texture,
            aux_data_view,
            aux_data_texture_height,
            glyph_data_texture,
            glyph_data_view,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn create_bind_groups(
        device: &wgpu::Device,
        layouts: &BindGroupLayouts,
        buffers: &Buffers,
        glyph_atlas_view: &wgpu::TextureView,
        color_glyph_atlas_view: &wgpu::TextureView,
        glyph_sampler: &wgpu::Sampler,
        gradient_texture_cache: &GradientTextureCache,
        path_image_view: &wgpu::TextureView,
        path_image_sampler: &wgpu::Sampler,
        has_storage_buffers: bool,
    ) -> BindGroups {
        // Binding 1: primitives (storage buffer or data texture)
        let binding_1 = if has_storage_buffers {
            wgpu::BindGroupEntry {
                binding: 1,
                resource: buffers.primitives.as_entire_binding(),
            }
        } else {
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    buffers
                        .prim_data_view
                        .as_ref()
                        .expect("DT mode requires prim_data_view"),
                ),
            }
        };

        // Binding 5: aux data (storage buffer or data texture)
        let binding_5 = if has_storage_buffers {
            wgpu::BindGroupEntry {
                binding: 5,
                resource: buffers.aux_data.as_entire_binding(),
            }
        } else {
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(
                    buffers
                        .aux_data_view
                        .as_ref()
                        .expect("DT mode requires aux_data_view"),
                ),
            }
        };

        let sdf = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF Bind Group"),
            layout: &layouts.sdf,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers.uniforms.as_entire_binding(),
                },
                binding_1,
                // Glyph atlas texture (binding 2)
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(glyph_atlas_view),
                },
                // Glyph sampler (binding 3)
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(glyph_sampler),
                },
                // Color glyph atlas texture (binding 4)
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(color_glyph_atlas_view),
                },
                binding_5,
            ],
        });

        // Path bind group (with gradient texture, image texture, and backdrop for glass)
        let path = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Path Bind Group"),
            layout: &layouts.path,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers.path_uniforms.as_entire_binding(),
                },
                // Gradient texture (binding 1)
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&gradient_texture_cache.view),
                },
                // Gradient sampler (binding 2)
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&gradient_texture_cache.sampler),
                },
                // Image texture (binding 3)
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(path_image_view),
                },
                // Image sampler (binding 4)
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(path_image_sampler),
                },
                // Backdrop texture (binding 5) - uses placeholder, will be replaced when glass is enabled
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(path_image_view),
                },
                // Backdrop sampler (binding 6)
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(path_image_sampler),
                },
                // Aux data (binding 7) — shared with the SDF pipeline.
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: if has_storage_buffers {
                        buffers.aux_data.as_entire_binding()
                    } else {
                        wgpu::BindingResource::TextureView(
                            buffers
                                .aux_data_view
                                .as_ref()
                                .expect("DT auxiliary texture"),
                        )
                    },
                },
            ],
        });

        // Glass bind group will be created when we have a backdrop texture
        BindGroups {
            sdf,
            glass: None,
            path,
        }
    }

    /// Update auxiliary data buffer (for 3D group shapes, polygon clips, etc.)
    ///
    /// If the batch has aux_data, writes it to the GPU buffer, recreating the buffer
    /// and rebinding if it's too small.
    pub(super) fn update_aux_data_buffer(&mut self, batch: &PrimitiveBatch) {
        self.update_aux_data_slice(&batch.aux_data);
    }

    /// Public wrapper for callers in `blinc_app` that need to
    /// re-upload a batch's aux_data after another batch's render
    /// pass clobbered the shared GPU buffer. Used by the
    /// non-compositor path's dynamic-batch dispatch: after the
    /// motion-bound batch's aux upload + draw, the static batch's
    /// polygon clip / 3D-group descriptor offsets would otherwise
    /// reference stale data on downstream dispatches.
    pub fn update_aux_data_for_batch(&mut self, batch: &PrimitiveBatch) {
        self.update_aux_data_buffer(batch);
    }

    /// Slice-variant of `update_aux_data_buffer` (private, batch-
    /// keyed) for callers that only have an `&[[f32; 4]]` — e.g.
    /// the compositor overlay path which carries the dynamic
    /// batch's aux_data separately from a `PrimitiveBatch`. Avoids
    /// constructing a throwaway batch just to satisfy the buffer-
    /// variant signature.
    pub fn update_aux_data_slice(&mut self, aux_data: &[[f32; 4]]) {
        if aux_data.is_empty() {
            return;
        }

        if !self.has_storage_buffers {
            // DT mode: upload aux data to texture instead of storage buffer
            self.update_aux_data_texture(aux_data);
            return;
        }

        let data_size = std::mem::size_of_val(aux_data) as u64;
        let buffer_size = self.buffers.aux_data.size();

        // Recreate buffer if too small
        if data_size > buffer_size {
            self.buffers.aux_data = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Aux Data Buffer"),
                size: data_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // Must recreate the SDF and path bind groups since the
            // buffer changed — path binding(7) references aux_data too
            // so the same handle-invalidation applies.
            self.rebind_sdf_bind_group();
            self.rebind_path_bind_group();
        }

        self.queue
            .write_buffer(&self.buffers.aux_data, 0, bytemuck::cast_slice(aux_data));
    }

    /// Upload auxiliary data to the DT fallback texture (Tier 3 / WebGL2).
    ///
    /// The texture has width=1024 and variable height. If the data exceeds
    /// the current texture capacity, the texture is recreated larger and
    /// the SDF bind group is rebound.
    pub(super) fn update_aux_data_texture(&mut self, aux_data: &[[f32; 4]]) {
        const AUX_TEX_WIDTH: u32 = 1024;
        let count = aux_data.len() as u32;
        let needed_height = count.div_ceil(AUX_TEX_WIDTH).max(1);

        if needed_height > self.buffers.aux_data_texture_height {
            // Recreate the texture with more rows
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Aux Data Texture"),
                size: wgpu::Extent3d {
                    width: AUX_TEX_WIDTH,
                    height: needed_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.buffers.aux_data_texture = Some(tex);
            self.buffers.aux_data_view = Some(view);
            self.buffers.aux_data_texture_height = needed_height;

            // Rebind since the texture changed
            self.rebind_sdf_bind_group();
            self.rebind_path_bind_group();
        }

        if let Some(ref tex) = self.buffers.aux_data_texture {
            // Pad aux_data to full rows so write_texture gets a complete rectangle
            let total_texels = (AUX_TEX_WIDTH * needed_height) as usize;
            let mut padded = aux_data.to_vec();
            padded.resize(total_texels, [0.0f32; 4]);

            let bytes = bytemuck::cast_slice::<[f32; 4], u8>(&padded);
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
                    bytes_per_row: Some(AUX_TEX_WIDTH * 16), // 1024 texels × 16 bytes
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: AUX_TEX_WIDTH,
                    height: needed_height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// Recreate the path bind group. Needed when the aux_data storage
    /// buffer is resized (binding 7 references the buffer by handle,
    /// so the old bind group keeps pointing at the freed buffer). Uses
    /// the placeholder path-image view — glass backdrops are applied
    /// through a separate bind group set that carries the real
    /// backdrop texture, so binding 5 here stays a placeholder.
    pub(super) fn rebind_path_bind_group(&mut self) {
        self.bind_groups.path = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Path Bind Group (rebound)"),
            layout: &self.bind_group_layouts.path,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.buffers.path_uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.gradient_texture_cache.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.gradient_texture_cache.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.placeholder_path_image_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&self.placeholder_path_image_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: if self.has_storage_buffers {
                        self.buffers.aux_data.as_entire_binding()
                    } else {
                        wgpu::BindingResource::TextureView(
                            self.buffers
                                .aux_data_view
                                .as_ref()
                                .expect("DT auxiliary texture"),
                        )
                    },
                },
            ],
        });
    }

    /// Recreate the SDF bind group (needed when aux_data buffer is resized).
    ///
    /// Uses the real glyph atlas if `active_glyph_atlas` is set, otherwise
    /// falls back to placeholder textures.
    pub(super) fn rebind_sdf_bind_group(&mut self) {
        // SAFETY: When active_glyph_atlas is Some, the pointers are valid for the
        // duration of the frame (they point to TextureViews owned by the text context).
        let (atlas_view, color_atlas_view): (&wgpu::TextureView, &wgpu::TextureView) =
            if let Some(active) = &self.active_glyph_atlas {
                unsafe { (&*active.atlas_view_ptr, &*active.color_atlas_view_ptr) }
            } else {
                (
                    &self.placeholder_glyph_atlas_view,
                    &self.placeholder_color_glyph_atlas_view,
                )
            };

        // Binding 1: primitives (storage buffer or data texture)
        let binding_1 = if self.has_storage_buffers {
            wgpu::BindGroupEntry {
                binding: 1,
                resource: self.buffers.primitives.as_entire_binding(),
            }
        } else {
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    self.buffers
                        .prim_data_view
                        .as_ref()
                        .expect("DT mode requires prim_data_view"),
                ),
            }
        };

        // Binding 5: aux data (storage buffer or data texture)
        let binding_5 = if self.has_storage_buffers {
            wgpu::BindGroupEntry {
                binding: 5,
                resource: self.buffers.aux_data.as_entire_binding(),
            }
        } else {
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(
                    self.buffers
                        .aux_data_view
                        .as_ref()
                        .expect("DT mode requires aux_data_view"),
                ),
            }
        };

        self.bind_groups.sdf = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF Bind Group (rebound)"),
            layout: &self.bind_group_layouts.sdf,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.buffers.uniforms.as_entire_binding(),
                },
                binding_1,
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.glyph_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(color_atlas_view),
                },
                binding_5,
            ],
        });
    }

    /// Update path vertex and index buffers
    pub(super) fn update_path_buffers(&mut self, batch: &PrimitiveBatch) {
        // Upload gradient texture if needed for multi-stop gradients
        if batch.paths.use_gradient_texture
            && let Some(ref stops) = batch.paths.gradient_stops
        {
            self.gradient_texture_cache.upload_stops(
                &self.queue,
                stops,
                crate::gradient_texture::SpreadMode::Pad,
            );
        }

        // Update path uniforms with clip data and brush metadata from batch
        let path_uniforms = PathUniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            clip_bounds: batch.paths.clip_bounds,
            clip_radius: batch.paths.clip_radius,
            clip_type: batch.paths.clip_type,
            use_gradient_texture: if batch.paths.use_gradient_texture {
                1
            } else {
                0
            },
            use_image_texture: if batch.paths.use_image_texture { 1 } else { 0 },
            use_glass_effect: if batch.paths.use_glass_effect { 1 } else { 0 },
            image_uv_bounds: batch.paths.image_uv_bounds,
            glass_params: batch.paths.glass_params,
            glass_tint: batch.paths.glass_tint,
            ..PathUniforms::default()
        };
        self.queue.write_buffer(
            &self.buffers.path_uniforms,
            0,
            bytemuck::bytes_of(&path_uniforms),
        );

        // Create or recreate vertex buffer if needed
        let vertex_size = (std::mem::size_of::<PathVertex>() * batch.paths.vertices.len()) as u64;
        let need_new_vertex_buffer = match &self.buffers.path_vertices {
            Some(buf) => buf.size() < vertex_size,
            None => true,
        };

        if need_new_vertex_buffer && vertex_size > 0 {
            self.buffers.path_vertices = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Path Vertex Buffer"),
                size: vertex_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        if let Some(vb) = &self.buffers.path_vertices {
            self.queue
                .write_buffer(vb, 0, bytemuck::cast_slice(&batch.paths.vertices));
        }

        // Create or recreate index buffer if needed
        let index_size = (std::mem::size_of::<u32>() * batch.paths.indices.len()) as u64;
        let need_new_index_buffer = match &self.buffers.path_indices {
            Some(buf) => buf.size() < index_size,
            None => true,
        };

        if need_new_index_buffer && index_size > 0 {
            self.buffers.path_indices = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Path Index Buffer"),
                size: index_size,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        if let Some(ib) = &self.buffers.path_indices {
            self.queue
                .write_buffer(ib, 0, bytemuck::cast_slice(&batch.paths.indices));
        }
    }
}
