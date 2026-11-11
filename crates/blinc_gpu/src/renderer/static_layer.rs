//! The cached static layer: creating, validating and invalidating it, drawing into it and compositing it.

use super::{
    EffectLayerRange, GpuRenderer, LayerStackFrame, SdfPrimitiveRanges, StaticLayer,
    aabb_intersects,
};
use crate::path::PathVertex;
use crate::primitives::{GpuPrimitive, PrimitiveBatch, Uniforms};

impl GpuRenderer {
    /// Render primitives with a specified clear color
    /// Ensure the layer-compositor's static-cache texture exists at
    /// the given dimensions and matches the surface format. Allocates
    /// on first use; reallocates on viewport resize. Marks the new
    /// texture invalid so the next `render_with_clear` knows it has
    /// to repaint the cache from scratch.
    pub fn ensure_static_layer(&mut self, width: u32, height: u32) {
        if let Some(layer) = &self.static_layer
            && layer.width == width
            && layer.height == height
        {
            return;
        }
        if width == 0 || height == 0 {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blinc-static-layer"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture_format,
            // RENDER_ATTACHMENT so we can render into it; COPY_SRC so
            // we can `copy_texture_to_texture` it onto the surface
            // each compositor frame.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("blinc-static-layer-view"),
            ..Default::default()
        });
        self.static_layer = Some(StaticLayer {
            texture,
            view,
            width,
            height,
            valid: false,
        });
    }

    /// Mark the static-cache texture invalid. Called when the
    /// composition source changes (rebuild, layout change, CSS state
    /// flip, scroll, motion-binding tick) — the next compositor
    /// frame will repaint the cache before using it.
    pub fn invalidate_static_layer(&mut self) {
        if let Some(layer) = self.static_layer.as_mut() {
            layer.valid = false;
        }
    }

    /// `true` if a static-cache texture exists and was painted into
    /// since the last invalidation. The compositor's fast path
    /// blits the cache when this is true; otherwise it falls back to
    /// rendering the cache afresh.
    pub fn static_layer_valid(&self) -> bool {
        self.static_layer.as_ref().map(|l| l.valid).unwrap_or(false)
    }

    /// Borrow the static-layer view, if the cache has been
    /// allocated. The compositor uses this as the render target for
    /// the static paint pass (everything goes into the offscreen
    /// texture instead of the surface).
    pub fn static_layer_view(&self) -> Option<&wgpu::TextureView> {
        self.static_layer.as_ref().map(|l| &l.view)
    }

    /// Mark the static-layer cache freshly painted. Called by the
    /// compositor after `render_static_layer` (or after an inner
    /// `render_with_clear` that targeted the static layer view)
    /// completes — flips `static_layer_valid()` to true so the next
    /// fast-path frame blits instead of repainting.
    pub fn mark_static_layer_valid(&mut self) {
        if let Some(layer) = self.static_layer.as_mut() {
            layer.valid = true;
        }
    }

    /// Render `batch` into the static-cache texture. The caller is
    /// expected to have called `ensure_static_layer` first; this
    /// method asserts in debug and no-ops in release if the cache is
    /// missing. After this returns, [`Self::static_layer_valid`]
    /// reads `true` and [`Self::blit_static_layer_into`] will copy
    /// the freshly-painted cache onto the surface.
    pub fn render_static_layer(&mut self, batch: &PrimitiveBatch, clear_color: [f64; 4]) {
        // Take the view out as a clone-of-handle so we can pass it
        // to render_with_clear without holding a borrow into
        // self.static_layer (render_with_clear needs `&mut self`).
        let view = match &self.static_layer {
            Some(layer) => layer.view.clone(),
            None => {
                debug_assert!(false, "render_static_layer without ensure_static_layer");
                return;
            }
        };
        self.render_with_clear(&view, batch, clear_color);
        if let Some(layer) = self.static_layer.as_mut() {
            layer.valid = true;
        }
    }

    /// Re-render the static layer only within `damage_rects`. Pixels
    /// outside the union are untouched; pixels inside are cleared and
    /// re-drawn from every primitive in `batch` whose AABB intersects
    /// the union. The static layer stays `valid` afterwards.
    ///
    /// Returns `false` when the cache isn't valid or non-SDF content
    /// the damage path doesn't yet support (paths, 3D viewports,
    /// particles) is present in the batch — caller falls back to a
    /// full repaint.
    ///
    /// Layer commands ARE supported (Phase 4d Opt 2): batches with
    /// push / pop pairs walk the same effect-layer logic that
    /// `render_with_layer_effects` uses, then composite each effect
    /// layer back into the static cache with `pending_damage_scissor`
    /// set so the blit lands only inside the damage union. Layers
    /// whose bounds don't intersect the damage union are skipped
    /// entirely.
    pub fn render_static_layer_damaged(
        &mut self,
        damage_rects: &[[f32; 4]],
        batch: &PrimitiveBatch,
    ) -> bool {
        let Some(layer) = self.static_layer.as_ref() else {
            return false;
        };
        if !layer.valid {
            return false;
        }
        // Bail to full repaint when the batch has anything the
        // damage path doesn't yet support. Paths / 3D viewports /
        // particles need their own scissored dispatch routines and
        // are out of scope for this commit; layer_commands are
        // handled below.
        if batch.has_ordered_draws()
            || batch.has_foreground_content()
            || batch.has_foreground_layers()
            || !batch.paths.vertices.is_empty()
            || !batch.viewports_3d.is_empty()
            || !batch.particle_viewports.is_empty()
        {
            return false;
        }
        if damage_rects.is_empty() {
            // No damage — caller can skip the dispatch entirely.
            return true;
        }

        let view = layer.view.clone();
        let layer_width = layer.width as f32;
        let layer_height = layer.height as f32;

        // Union all damage rects into a single scissor (wgpu only
        // supports one scissor per pass). Clamp to layer extent —
        // a damage rect can extend past the cache when a binding's
        // primitives moved off-screen.
        //
        // Pad outward by `AABB_PAD` pixels. SDF primitives anti-
        // alias at the edges of their `bounds`; without padding,
        // 1-pixel AA pixels at the trailing edge of the moved
        // element stay in the cache from the previous frame and
        // produce a faint "vibrating" outline as the bounds round
        // sub-pixel positions differently each frame. 4 px is well
        // over the AA distance for typical UI font sizes and
        // corner radii (~1-2 px) while still keeping the damage
        // rect proportional to the actual movement.
        const AABB_PAD: f32 = 4.0;
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for [x, y, w, h] in damage_rects.iter().copied() {
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            min_x = min_x.min(x - AABB_PAD);
            min_y = min_y.min(y - AABB_PAD);
            max_x = max_x.max(x + w + AABB_PAD);
            max_y = max_y.max(y + h + AABB_PAD);
        }
        if !min_x.is_finite() || max_x <= min_x || max_y <= min_y {
            return true;
        }
        let scissor_x = min_x.max(0.0).floor() as u32;
        let scissor_y = min_y.max(0.0).floor() as u32;
        let scissor_right = max_x.min(layer_width).ceil() as u32;
        let scissor_bottom = max_y.min(layer_height).ceil() as u32;
        if scissor_right <= scissor_x || scissor_bottom <= scissor_y {
            return true;
        }
        let scissor_w = scissor_right - scissor_x;
        let scissor_h = scissor_bottom - scissor_y;

        // Phase 4d Opt 2: build the effect-layer list from
        // `batch.layer_commands` (mirrors `render_with_layer_effects`
        // exactly, including the Phase 4a relax that lets pure-
        // opacity layers reach the offscreen-composite path). Each
        // entry's primitive range gets excluded from the SDF pass
        // below — those primitives render into their layer's tight
        // texture instead. The layer-composite blit lands inside
        // the damage scissor via `pending_damage_scissor`.
        let mut effect_layers: Vec<EffectLayerRange> = Vec::new();
        let mut effect_primitive_indices: std::collections::HashSet<usize> =
            std::collections::HashSet::new();
        if !batch.layer_commands.is_empty() {
            use crate::primitives::LayerCommand;
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
            for layer in &effect_layers {
                for i in layer.primitive_start..layer.primitive_end {
                    effect_primitive_indices.insert(i);
                }
            }
        }

        // Cull primitives to only those intersecting the union rect
        // AND not inside an effect layer. The static cache outside
        // the scissor is preserved by `LoadOp::Load`, so we skip
        // every prim whose AABB doesn't touch the damaged region.
        // Effect-layer prims render separately into their layer's
        // offscreen texture below.
        let scissor_rect = [
            scissor_x as f32,
            scissor_y as f32,
            scissor_w as f32,
            scissor_h as f32,
        ];
        let visible: Vec<GpuPrimitive> = batch
            .primitives
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                if effect_primitive_indices.contains(i) {
                    return false;
                }
                let [x, y, w, h] = p.bounds;
                if w <= 0.0 || h <= 0.0 {
                    return false;
                }
                aabb_intersects(scissor_rect, [x, y, w, h])
            })
            .map(|(_, p)| p)
            .copied()
            .collect();

        // Update uniforms (viewport is the cache extent, not the
        // window — primitives were emitted into cache pixel space).
        let uniforms = Uniforms {
            viewport_size: [layer_width, layer_height],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        let sdf_ranges = if visible.is_empty() {
            SdfPrimitiveRanges::default()
        } else {
            self.upload_sorted_primitives(&visible)
        };
        self.update_aux_data_buffer(batch);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blinc-damage-rect-encoder"),
            });

        // Single-pass damage-rect re-render. wgpu's `LoadOp::Clear`
        // ignores `set_scissor_rect`, so we can't clear-inside-
        // scissor via attachment ops. Instead we open ONE render
        // pass with `LoadOp::Load` and issue two draws inside it:
        //
        //   1. clear-quad pipeline (REPLACE blend, writes opaque
        //      black) → with the active scissor, zeros the damaged
        //      region.
        //   2. SDF pipeline → re-paints the cleared region in
        //      z-order from every primitive whose AABB intersects
        //      the union damage rect.
        //
        // Two passes was the original cut but every render-pass
        // begin/end forces a tile-memory load + store on Metal
        // (the static cache is a multi-MB texture). Folding both
        // draws into a single pass means one load + one store per
        // damage frame instead of two — measurable on cn_demo's
        // switch / progress / slider animations where the slow
        // path's single-pass full-clear was visibly smoother than
        // the two-pass damage path despite doing more total work.
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blinc-damage-rect-combined"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_w, scissor_h);
            // First draw: scissored clear via the clear-quad pipeline.
            render_pass.set_pipeline(&self.pipelines.clear_quad);
            render_pass.draw(0..3, 0..1);
            // Second draw: SDF re-paint inside the just-cleared scissor.
            if !visible.is_empty() {
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

        self.queue.submit(std::iter::once(encoder.finish()));

        // Phase 4d Opt 2: composite each effect layer back into the
        // static cache with `pending_damage_scissor` set so the blit
        // lands only inside the damage union. Layers whose bounds
        // don't intersect the damage rect are skipped entirely —
        // their previous-frame pixels stay in the cache (preserved
        // by `LoadOp::Load` in the SDF pass above).
        //
        // The blit's own visible-bounds scissor intersects with
        // `pending_damage_scissor` inside `blit_tight_texture_to_target`
        // (commit e0286ef0). Empty intersection ⇒ the blit's render
        // pass submits as a no-op.
        if !effect_layers.is_empty() {
            let damage_scissor = (scissor_x, scissor_y, scissor_w, scissor_h);
            let damage_rect_f = [
                scissor_x as f32,
                scissor_y as f32,
                (scissor_x + scissor_w) as f32,
                (scissor_y + scissor_h) as f32,
            ];
            for layer in effect_layers {
                let primitives = &batch.primitives[layer.primitive_start..layer.primitive_end];
                let path_verts: &[PathVertex] = if layer.path_vertex_end > layer.path_vertex_start {
                    &batch.paths.vertices[layer.path_vertex_start..layer.path_vertex_end]
                } else {
                    &[]
                };
                if primitives.is_empty() && path_verts.is_empty() {
                    continue;
                }

                // Compute layer's screen-space bounding box (same
                // logic as `render_with_layer_effects`). Path vertex
                // positions matter for Lottie-style content; SDF
                // primitives carry their bbox in `bounds`.
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
                let layer_width = (max_x - min_x).max(1.0);
                let layer_height = (max_y - min_y).max(1.0);
                let layer_pos = (min_x, min_y);
                let layer_size = (layer_width, layer_height);
                let layer_clip = clip;

                // Layer ∩ damage check — skip if disjoint. Compare
                // pre-effect-expansion bounds (post-expansion bounds
                // get computed below for the blit destination, but
                // the *content* lives inside the raw bounds and an
                // empty raw ∩ damage means no work needed).
                if layer_pos.0 + layer_size.0 <= damage_rect_f[0]
                    || layer_pos.1 + layer_size.1 <= damage_rect_f[1]
                    || layer_pos.0 >= damage_rect_f[2]
                    || layer_pos.1 >= damage_rect_f[3]
                {
                    continue;
                }

                // Skip if entirely outside viewport (same as
                // `render_with_layer_effects`).
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

                let config = layer.config.clone();
                let effect_expansion = Self::calculate_effect_expansion(&config.effects);

                // Render layer to tight offscreen texture (helper
                // already exists; same call shape as
                // `render_with_layer_effects`).
                let (layer_texture, content_size) = self.render_layer_range_tight(
                    batch,
                    layer.primitive_start,
                    layer.primitive_end,
                    layer.path_vertex_start,
                    layer.path_vertex_end,
                    layer.path_index_start,
                    layer.path_index_end,
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

                // The damage scissor is the intersection rule:
                // `blit_tight_texture_to_target` intersects its own
                // visible-bounds scissor with this, so the blit
                // paints only inside (layer content area) ∩ (damage
                // rect). Empty intersection ⇒ the blit is a no-op.
                self.set_pending_damage_scissor(damage_scissor);

                if config.effects.is_empty() {
                    self.blit_tight_texture_to_target(
                        &layer_texture.view,
                        tight_size,
                        &view,
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
                    let effected = self.apply_layer_effects(&layer_texture, &config.effects);
                    self.layer_texture_cache.release(layer_texture);
                    self.blit_tight_texture_to_target(
                        &effected.view,
                        tight_size,
                        &view,
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

                self.clear_pending_damage_scissor();
            }
        }

        true
    }

    /// `copy_texture_to_texture` the static-cache onto the supplied
    /// surface texture. Caller must use the underlying `wgpu::Texture`
    /// (not a `TextureView`) because that's what `CommandEncoder::
    /// copy_texture_to_texture` requires. Returns `false` if the
    /// cache hasn't been painted yet — in that case the caller
    /// should fall back to a full repaint.
    pub fn blit_static_layer_into(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target_texture: &wgpu::Texture,
    ) -> bool {
        let Some(layer) = &self.static_layer else {
            return false;
        };
        if !layer.valid {
            return false;
        }
        let extent = wgpu::Extent3d {
            width: layer.width,
            height: layer.height,
            depth_or_array_layers: 1,
        };
        encoder.copy_texture_to_texture(
            layer.texture.as_image_copy(),
            target_texture.as_image_copy(),
            extent,
        );
        true
    }

    /// Layer-compositor composite step: blit the cached static
    /// layer onto the surface and then dispatch `overlay_primitives`
    /// (typically canvas content re-emitted this frame) on top.
    ///
    /// Done in a single command-encoder submission so the GPU sees
    /// blit-then-load-then-draw as one ordered batch — no extra
    /// submit overhead vs the previous full-paint path.
    ///
    /// Returns `false` if the static layer hasn't been painted yet;
    /// the caller should fall back to a full repaint into the cache
    /// before retrying.
    pub fn composite_frame(
        &mut self,
        target_view: &wgpu::TextureView,
        target_texture: &wgpu::Texture,
        overlay_primitives: &[GpuPrimitive],
        overlay_aux_data: &[[f32; 4]],
    ) -> bool {
        // First, validate the cache and capture the extent. We can't
        // hold a borrow into `self.static_layer` across the upload
        // calls below (they take `&mut self`), so we just record what
        // we need.
        let extent = match &self.static_layer {
            Some(layer) if layer.valid => wgpu::Extent3d {
                width: layer.width,
                height: layer.height,
                depth_or_array_layers: 1,
            },
            _ => return false,
        };

        // Upload + sort overlay primitives (smaller than the full
        // batch — typically <200 prims for cn_demo's spinners). The
        // SDF GPU buffer is shared with the static pass that wrote
        // earlier this frame, but the GPU has already consumed
        // those reads in the static-layer render pass, so the queue
        // is allowed to overwrite the buffer here.
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Upload the overlay's aux_data so polygon-clip / 3D-group
        // offsets carried on overlay primitives index into valid GPU
        // data. The static layer's aux_data was written earlier this
        // frame but has already been consumed into the cache texture;
        // overwriting the buffer here is safe and ensures the overlay
        // SDF dispatch reads the right vertex / shape descriptors.
        self.update_aux_data_slice(overlay_aux_data);

        let visible_overlay: Vec<GpuPrimitive> = overlay_primitives
            .iter()
            .filter(|p| self.is_primitive_visible(p))
            .copied()
            .collect();
        let sdf_ranges = if visible_overlay.is_empty() {
            SdfPrimitiveRanges::default()
        } else {
            self.upload_sorted_primitives(&visible_overlay)
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blinc-compositor-encoder"),
            });

        // 1. Blit static cache onto surface (replaces all pixels).
        let source_copy = self.static_layer.as_ref().unwrap().texture.as_image_copy();
        let dest_copy = target_texture.as_image_copy();
        encoder.copy_texture_to_texture(source_copy, dest_copy, extent);

        // 2. Draw overlay primitives on top with `LoadOp::Load` so
        //    the just-blitted cache content is preserved.
        if !visible_overlay.is_empty() {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blinc-compositor-overlay-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
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
        true
    }
}
