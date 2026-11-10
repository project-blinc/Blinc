//! Rendering a frame: the entry points, clearing, culling and the multisampled path.

use super::{EffectLayerRange, GpuRenderer, LayerStackFrame};
use crate::primitives::{GpuPrimitive, PrimitiveBatch, Uniforms};

impl GpuRenderer {
    /// Render a batch of primitives to a texture view
    /// Render primitives with transparent background (default)
    pub fn render(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        self.render_with_clear(target, batch, [0.0, 0.0, 0.0, 0.0]);
    }

    /// Render primitives at a specific viewport size (for reduced-resolution rendering)
    ///
    /// Used for glass backdrop rendering at half resolution.
    pub fn render_at_size(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
        width: u32,
        height: u32,
    ) {
        // Temporarily override viewport size for this render
        let original_size = self.viewport_size;
        self.viewport_size = (width, height);
        self.render_with_clear(target, batch, clear_color);
        self.viewport_size = original_size;
    }

    ///
    /// # Arguments
    /// * `target` - The texture view to render to
    /// * `batch` - The primitive batch to render
    /// * `clear_color` - RGBA clear color (0.0-1.0 range)
    pub fn render_with_clear(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
    ) {
        if batch.has_ordered_draws() || Self::has_nested_layers(batch) {
            self.render_with_clear_simple(target, &PrimitiveBatch::new(), clear_color);
            self.render_layered_overlay_msaa(target, batch, 1);
            return;
        }

        // Evict oversized textures from the pool at frame start
        // This prevents memory bloat from accumulated large textures
        self.layer_texture_cache.evict_oversized();

        // Check whether any layer command needs the layer-aware path.
        // Phase 4a: include pure-opacity layers so `config.opacity`
        // actually reaches the composite blit. Without this, opacity-
        // only `@keyframes` rendered at full alpha (the simple path
        // ignores layer commands entirely).
        let has_layer_effects = batch.layer_commands.iter().any(|entry| {
            if let crate::primitives::LayerCommand::Push { config } = &entry.command {
                !config.effects.is_empty()
                    || config.blend_mode != blinc_core::BlendMode::Normal
                    || config.transform_3d.is_some()
                    || config.opacity < 1.0
            } else {
                false
            }
        });

        tracing::trace!(
            "render_with_clear: {} primitives, {} layer commands, has_layer_effects={}",
            batch.primitives.len(),
            batch.layer_commands.len(),
            has_layer_effects
        );

        // If we have layer effects, use the layer-aware rendering path
        if has_layer_effects {
            self.render_with_layer_effects(target, batch, clear_color);
            return;
        }

        // Standard rendering (no layer effects)
        self.render_with_clear_simple(target, batch, clear_color);
    }

    /// Simple render with clear (no layer effect processing)
    /// Test whether a primitive's expanded bounds intersect the viewport.
    ///
    /// Returns `true` if the primitive might be visible and should be rendered.
    /// Conservative: accounts for shadow, border, and rotation expansion.
    /// Primitives with 3D perspective are always considered visible.
    #[inline]
    pub(super) fn is_primitive_visible(&self, prim: &GpuPrimitive) -> bool {
        let vp_w = self.viewport_size.0 as f32;
        let vp_h = self.viewport_size.1 as f32;

        let px = prim.bounds[0];
        let py = prim.bounds[1];
        let pw = prim.bounds[2];
        let ph = prim.bounds[3];

        // Primitives with 3D perspective may project anywhere
        if prim.perspective[2] > 0.0 {
            return true;
        }

        // Text glyph primitives (prim_type 7) bypass the
        // viewport AABB cull. A scroll / layer container translates
        // its children at composite time, so glyph bounds live in
        // content (pre-translate) space — a glyph at content y=2000
        // that's visible on screen after a -1000 scroll would otherwise
        // get dropped here. The fragment shader's clip / alpha already
        // discards fragments outside the layer bounds, so skipping this
        // cull is cheap: the primitive either clips to nothing (free)
        // or renders at the correct place.
        if prim.type_info[0] == 7 {
            return true;
        }

        // Account for shadow expansion (matches shader bounds computation)
        let shadow_blur = prim.shadow[2];
        let shadow_ox = prim.shadow[0].abs();
        let shadow_oy = prim.shadow[1].abs();
        let mut expand = shadow_blur * 3.0 + shadow_ox + shadow_oy;

        // Account for border (stroke) expansion
        expand += prim.border[0];

        // Account for rotation — rotated rects have larger AABB
        // rotation = [sin_rz, cos_rz, sin_ry, cos_ry], identity = [0, 1, 0, 1]
        let has_rotation = prim.rotation[0] != 0.0 || prim.rotation[2] != 0.0;
        if has_rotation {
            // Worst case AABB expansion for rotated rect: half-diagonal
            let half_diag = (pw * pw + ph * ph).sqrt() * 0.5;
            expand += half_diag;
        }

        // Non-identity local affine — be generous with expansion
        let has_affine = prim.local_affine[1] != 0.0 || prim.local_affine[2] != 0.0;
        if has_affine {
            let half_diag = (pw * pw + ph * ph).sqrt() * 0.5;
            expand += half_diag;
        }

        // AABB intersection with viewport [0, 0, vp_w, vp_h]
        let left = px - expand;
        let top = py - expand;
        let right = px + pw + expand;
        let bottom = py + ph + expand;

        right > 0.0 && bottom > 0.0 && left < vp_w && top < vp_h
    }

    /// Cull a slice of primitives, returning only those visible in the viewport.
    pub(super) fn cull_primitives(&self, prims: &[GpuPrimitive]) -> Vec<GpuPrimitive> {
        prims
            .iter()
            .filter(|p| self.is_primitive_visible(p))
            .copied()
            .collect()
    }

    pub(super) fn render_with_clear_simple(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
    ) {
        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Cull off-screen primitives before GPU upload
        let mut visible_primitives = self.cull_primitives(&batch.primitives);

        // A page background that fills the target and sits under everything else is the clear
        // colour: drawing it would shade every pixel once more for the same result.
        let backdrop = visible_primitives.first().and_then(|p| {
            p.opaque_backdrop((self.viewport_size.0 as f32, self.viewport_size.1 as f32))
        });
        let clear_color = match backdrop {
            Some(color) => {
                visible_primitives.remove(0);
                color
            }
            None => clear_color,
        };

        // Sort primitives by pipeline category and upload
        let sdf_ranges = self.upload_sorted_primitives(&visible_primitives);

        // Update auxiliary data buffer (group shapes, polygon clips)
        // This may call rebind_sdf_bind_group() if the buffer needs resizing.
        // When active_glyph_atlas is set, rebind uses the real atlas automatically.
        self.update_aux_data_buffer(batch);

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Render Encoder"),
            });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Render Pass"),
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

            // Render SDF primitives via split pipelines.
            if !visible_primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    false,
                    self.sdf_vb_buffer(),
                );
            }

            // Render tessellated paths in a single monolithic draw. This
            // places every path on top of every SDF primitive regardless
            // of submission order, which is the right default for most
            // Blinc UI (SVG icons over background cards, canvas fills
            // over SDF text, etc). For Lottie scenes whose layer stack
            // interleaves text (SDF primitive stream via `draw_text`)
            // with shape fills (tessellated path pipeline), this order
            // can bury text under later shape fills even though Lottie's
            // back-to-front order would put the text on top. The
            // follow-up is to route path fills through the SDF pipeline
            // as a new primitive type so the existing submission-order
            // dispatch handles them the same way it handles text — see
            // BACKLOG.md for the plan.
            if has_paths
                && let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
            {
                render_pass.set_pipeline(&self.pipelines.path);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));

        // Render SDF 3D viewports (after main content, so they render on top)
        if !batch.viewports_3d.is_empty() {
            self.render_sdf_3d_viewports(target, &batch.viewports_3d);
        }

        // Render GPU particle viewports (after SDF viewports)
        if !batch.particle_viewports.is_empty() {
            self.render_particle_viewports(target, &batch.particle_viewports);
        }
    }

    /// Render with layer effect processing
    ///
    /// This implements a correct layer effect system:
    /// 1. Identify primitive ranges for effect layers
    /// 2. Render non-effect primitives to target (skipping those in effect layers)
    /// 3. For each effect layer, render to viewport-sized texture, apply effects, blit at position
    pub(super) fn render_with_layer_effects(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
    ) {
        use crate::primitives::LayerCommand;

        // Build list of effect layers with their primitive AND path
        // ranges. Tracking path ranges (not just primitive indices)
        // is what lets translucent-opacity Lottie layers — whose
        // shapes live entirely in `batch.paths` — reach the
        // offscreen composite instead of bypassing it.
        let mut effect_layers: Vec<EffectLayerRange> = Vec::new();
        let mut layer_stack: Vec<(LayerStackFrame, blinc_core::LayerConfig)> = Vec::new();

        for entry in &batch.layer_commands {
            match &entry.command {
                LayerCommand::Push { config } => {
                    layer_stack.push((
                        LayerStackFrame {
                            primitive_start: entry.primitive_index,
                            path_index_start: entry.path_index_count,
                            path_vertex_start: entry.path_vertex_index,
                        },
                        config.clone(),
                    ));
                }
                LayerCommand::Pop => {
                    if let Some((start, config)) = layer_stack.pop() {
                        // Phase 4a: include pure-opacity layers
                        // (`config.opacity < 1.0` alone). Pre-fix the
                        // gate required effects / blend / 3D; pure-
                        // opacity layers were silently dropped — their
                        // primitives rendered at full alpha in the
                        // first pass and `config.opacity` went
                        // nowhere. The walker's hybrid flatten now
                        // skips `push_layer` for simple-enough opacity
                        // subtrees (children.len() <= 1) so those
                        // don't reach this gate at all; what does
                        // reach is the cases that genuinely need a
                        // layer composite (overlapping children,
                        // nested opacity), and they get correct
                        // offscreen-then-composite rendering via
                        // `blit_tight_texture_to_target`, which reads
                        // `config.opacity`.
                        let has_effect_or_blend_or_3d = !config.effects.is_empty()
                            || config.blend_mode != blinc_core::BlendMode::Normal
                            || config.transform_3d.is_some();
                        let has_pure_opacity = config.opacity < 1.0;
                        if has_effect_or_blend_or_3d || has_pure_opacity {
                            effect_layers.push(EffectLayerRange {
                                primitive_start: start.primitive_start,
                                primitive_end: entry.primitive_index,
                                path_index_start: start.path_index_start,
                                path_index_end: entry.path_index_count,
                                path_vertex_start: start.path_vertex_start,
                                path_vertex_end: entry.path_vertex_index,
                                config,
                            });
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

        // If no effect layers, just render normally
        if effect_layers.is_empty() {
            self.render_with_clear_simple(target, batch, clear_color);
            return;
        }

        // Build set of primitive indices AND path-index ranges that
        // belong to effect layers so the first pass knows what to
        // skip. Paths are skipped as whole `[start..end)` ranges —
        // the draw call is split around them rather than using a
        // per-index hash set (index buffers are much larger than
        // the primitive count).
        let mut effect_primitives = std::collections::HashSet::new();
        let mut path_skip_ranges: Vec<(u32, u32)> = Vec::new();
        for layer in &effect_layers {
            for i in layer.primitive_start..layer.primitive_end {
                effect_primitives.insert(i);
            }
            if layer.path_index_end > layer.path_index_start {
                path_skip_ranges.push((layer.path_index_start as u32, layer.path_index_end as u32));
            }
        }
        path_skip_ranges.sort_unstable();

        // First pass: render primitives + paths that are NOT in effect layers
        self.render_primitives_excluding_with_paths(
            target,
            batch,
            &effect_primitives,
            &path_skip_ranges,
            clear_color,
        );
        drop(effect_primitives); // Free HashSet immediately - not needed after first pass

        // Process each effect layer
        for layer in effect_layers {
            let EffectLayerRange {
                primitive_start: start_idx,
                primitive_end: end_idx,
                config,
                ..
            } = &layer;
            let (start_idx, end_idx) = (*start_idx, *end_idx);
            let config = config.clone();
            let has_primitives = start_idx < end_idx && end_idx <= batch.primitives.len();
            let has_paths = layer.path_index_end > layer.path_index_start
                && layer.path_index_end <= batch.paths.indices.len();
            if !has_primitives && !has_paths {
                continue;
            }

            // Compute the layer's screen-space bounding box from its
            // primitives + path vertices. Lottie layers contain only
            // paths (tessellated by `fill_path`), no SDF primitives —
            // so the bbox MUST include path vertices, otherwise the
            // tight texture would be sized from the LayerConfig
            // fallback (often viewport-sized) and the offscreen blit
            // would be in the wrong position.
            let primitives = &batch.primitives[start_idx..end_idx];
            let path_verts = if has_paths {
                &batch.paths.vertices[layer.path_vertex_start..layer.path_vertex_end]
            } else {
                &[][..]
            };
            let (layer_pos, layer_size, layer_clip) = if primitives.is_empty()
                && path_verts.is_empty()
            {
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
                for v in path_verts {
                    min_x = min_x.min(v.position[0]);
                    min_y = min_y.min(v.position[1]);
                    max_x = max_x.max(v.position[0]);
                    max_y = max_y.max(v.position[1]);
                    if clip.is_none() && v.clip_bounds[0] > -5000.0 && v.clip_bounds[2] < 90000.0 {
                        clip = Some((v.clip_bounds, v.clip_radius));
                    }
                }
                let width = (max_x - min_x).max(1.0);
                let height = (max_y - min_y).max(1.0);
                ((min_x, min_y), (width, height), clip)
            };

            // Skip layers that are entirely outside the viewport
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

            // Calculate effect expansion (how much effects extend beyond original bounds)
            let effect_expansion = Self::calculate_effect_expansion(&config.effects);

            // Render layer primitives + paths to a TIGHT texture
            // (not viewport-sized!). Path range comes from the
            // recorded `LayerCommandEntry`, so Lottie shapes —
            // which only ever produce path geometry — get their
            // own slice composed at full alpha into the offscreen
            // before the blit applies the layer's opacity.
            let (layer_texture, content_size) = self.render_layer_range_tight(
                batch,
                start_idx,
                end_idx,
                layer.path_vertex_start,
                layer.path_vertex_end,
                layer.path_index_start,
                layer.path_index_end,
                layer_pos,
                layer_size,
                effect_expansion,
            );

            // Use content_size for blitting (not layer_texture.size which may be larger)
            let tight_size = content_size;

            // Calculate the destination position and size for blitting
            // Don't clamp to 0 - allow negative positions for scrolled content
            // The blit function will handle off-screen portions correctly
            let expanded_pos = (
                layer_pos.0 - effect_expansion.0,
                layer_pos.1 - effect_expansion.1,
            );
            let expanded_size = (
                layer_size.0 + effect_expansion.0 + effect_expansion.2,
                layer_size.1 + effect_expansion.1 + effect_expansion.3,
            );

            // Skip texture copy when no effects - use layer_texture directly
            if config.effects.is_empty() {
                // Blit directly without effect processing (skip copy)
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
                // Apply effects to the tight texture
                let effected = self.apply_layer_effects(&layer_texture, &config.effects);
                self.layer_texture_cache.release(layer_texture);

                // Blit the effected texture back to target at the correct position
                // Pass through the clip bounds so effects don't bleed outside scroll containers
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

    /// Render primitives excluding those in the given set
    pub(super) fn render_primitives_excluding(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        exclude: &std::collections::HashSet<usize>,
        clear_color: [f64; 4],
    ) {
        self.render_primitives_excluding_with_paths(target, batch, exclude, &[], clear_color)
    }

    /// Like [`Self::render_primitives_excluding`] but also skips a
    /// set of path-index ranges from the single path draw call. The
    /// ranges must be sorted by start index and non-overlapping —
    /// callers get that from `EffectLayerRange` values built by
    /// `render_with_layer_effects`, which emits layers in a
    /// stack-ordered, non-overlapping manner.
    pub(super) fn render_primitives_excluding_with_paths(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        exclude: &std::collections::HashSet<usize>,
        path_skip_ranges: &[(u32, u32)],
        clear_color: [f64; 4],
    ) {
        // If nothing to exclude, use simple path
        if exclude.is_empty() && path_skip_ranges.is_empty() {
            self.render_with_clear_simple(target, batch, clear_color);
            return;
        }

        // Build list of primitives to render (excluding effect layers + off-screen)
        let included_primitives: Vec<GpuPrimitive> = batch
            .primitives
            .iter()
            .enumerate()
            .filter(|(i, p)| !exclude.contains(i) && self.is_primitive_visible(p))
            .map(|(_, p)| *p)
            .collect();

        if included_primitives.is_empty() && batch.paths.vertices.is_empty() {
            // Just clear the target
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Clear Encoder"),
                });
            {
                let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear Pass"),
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
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            return;
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update auxiliary data buffer
        self.update_aux_data_buffer(batch);

        // Sort and upload filtered primitives
        let sdf_ranges = self.upload_sorted_primitives(&included_primitives);

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Filtered Render Encoder"),
            });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Filtered Render Pass"),
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

            // Render SDF primitives via split pipelines (filtered)
            if !included_primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    false,
                    self.sdf_vb_buffer(),
                );
            }

            // Render paths, skipping any index ranges that belong
            // to translucent / effect layers (those are composited
            // offscreen in the second pass). Paths in non-effect
            // ranges still render here directly to the target.
            if has_paths
                && let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
            {
                render_pass.set_pipeline(&self.pipelines.path);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                let total = batch.paths.indices.len() as u32;
                let mut cursor = 0u32;
                for &(skip_start, skip_end) in path_skip_ranges {
                    if skip_start > cursor {
                        render_pass.draw_indexed(cursor..skip_start, 0, 0..1);
                    }
                    cursor = cursor.max(skip_end);
                }
                if cursor < total {
                    render_pass.draw_indexed(cursor..total, 0, 0..1);
                }
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render primitives with MSAA (multi-sample anti-aliasing)
    ///
    /// # Arguments
    /// * `msaa_target` - The multisampled texture view to render to
    /// * `resolve_target` - The single-sampled texture view to resolve to
    /// * `batch` - The primitive batch to render
    /// * `clear_color` - RGBA clear color (0.0-1.0 range)
    pub fn render_msaa(
        &mut self,
        msaa_target: &wgpu::TextureView,
        resolve_target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
    ) {
        if batch.has_ordered_draws() {
            self.render_with_clear_simple(resolve_target, &PrimitiveBatch::new(), clear_color);
            self.render_layered_overlay_msaa(
                resolve_target,
                batch,
                msaa_target.texture().sample_count(),
            );
            return;
        }
        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Polygon clips and 3D groups read their vertices from here
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
                label: Some("Blinc MSAA Render Encoder"),
            });

        // Begin render pass with MSAA resolve
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc MSAA Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: msaa_target,
                    resolve_target: Some(resolve_target),
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

            // Render SDF primitives via split pipelines
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

            // Render paths
            if has_paths
                && let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
            {
                render_pass.set_pipeline(&self.pipelines.path);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
