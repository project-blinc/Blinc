//! Coordinate motion-frame collection, preparation and ordered dispatch.
use super::frame_dispatch::FrameContent;
use super::frame_text::PreparedFrameText;
use super::{
    DebugMode, DrawContext, GpuPaintContext, ImageElement, LayoutNodeId, PrimitiveBatch,
    RenderContext, RenderTree, Result, SvgElement, TextElement, Transform3DLayerInfo,
    composite_replay, dispatch_pending_gpu_passes, dispatch_pending_meshes,
};

impl RenderContext {
    #[allow(clippy::too_many_arguments)]
    pub fn render_tree_with_motion_opt(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
        target_texture: Option<&wgpu::Texture>,
        try_fast_paint: bool,
    ) -> Result<()> {
        // Consume the atlas-growth notification; frame preparation binds the current atlas
        // views.
        let _ = self.text_ctx.take_atlas_grew();

        // Use the static compositor when the surface texture is available. Inner cache
        // paints pass no texture.
        if let Some(texture) = target_texture {
            return self.try_render_with_compositor(
                tree,
                render_state,
                width,
                height,
                target,
                texture,
                try_fast_paint,
            );
        }
        // Trace time spent in each rendering stage.
        let p4_start = web_time::Instant::now();

        // Get scale factor for HiDPI rendering
        let scale_factor = tree.scale_factor();

        // Commit animation and promotion status for both direct and compositor rendering.
        let v2_statuses = tree.compute_animation_status();
        tree.commit_animation_status(&v2_statuses);

        // Refresh independent canvases and patch bindings when the cached batch permits it;
        // otherwise walk the tree.
        let used_fast_paint = try_fast_paint
            && self.redraw_canvases(tree, width, height)
            && self.apply_binding_deltas(tree, scale_factor);

        // Create a single paint context for all layers with text rendering support
        let mut ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);

        // A successful patch already contains this frame's geometry.
        if !used_fast_paint {
            tree.render_with_motion(&mut ctx, render_state);
        }
        let t_paint_walker = p4_start.elapsed();

        if !used_fast_paint {
            composite_replay::update_subtree_bakes(
                &mut self.renderer,
                tree,
                ctx.take_composite_layer_batches(),
                &mut self.css_composited_textures,
                &mut self.css_composited_scratch_hash,
                &mut self.motion_subtree_textures,
                &mut self.motion_subtree_scratch_hash,
            );
        }

        // Take the batch (mutable so CSS-transformed text primitives can be added).
        // Fast path: clone the patched cache; full path: take from ctx.
        let mut batch = if used_fast_paint {
            self.cached_bg_batch
                .as_ref()
                .cloned()
                .unwrap_or_else(|| ctx.take_batch())
        } else {
            ctx.take_batch()
        };

        // Replace dynamic content after a full walk; patched fast frames retain the
        // existing owner batch.
        if !used_fast_paint {
            self.cached_dynamic_batch = Some(ctx.take_dynamic_batch());
        }

        // Collect mesh output for dispatch after 2D content.
        let pending_meshes = ctx.take_pending_meshes();
        // Dispatch callback GPU passes alongside captured meshes after 2D content.
        let pending_gpu_passes = ctx.take_pending_gpu_passes();

        // Collect text, SVG, images and flows, reusing compatible cached element lists.
        let collect_start = web_time::Instant::now();
        // Refresh collected positions for active motion and direct-write drift. Pure
        // timeline rotation can keep unrelated collected content cached.
        let any_motion_active = render_state.has_active_motions()
            || tree.has_active_visual_animations()
            || tree.has_active_layout_animations()
            || tree
                .motion_bindings_map()
                .values()
                .any(|b| b.is_any_position_animating())
            || {
                let bindings_table = tree.motion_bindings_map();
                tree.composite_bindings().iter().any(|(node, meta)| {
                    let Some(b) = bindings_table.get(node) else {
                        return false;
                    };
                    let cx = b
                        .translate_x
                        .as_ref()
                        .and_then(|v| v.lock().ok())
                        .map(|g| g.get())
                        .unwrap_or(0.0);
                    let cy = b
                        .translate_y
                        .as_ref()
                        .and_then(|v| v.lock().ok())
                        .map(|g| g.get())
                        .unwrap_or(0.0);
                    if (cx - meta.last_translate.0).abs() > 0.5
                        || (cy - meta.last_translate.1).abs() > 0.5
                    {
                        return true;
                    }
                    if let Some(ref v) = b.opacity {
                        if let Ok(g) = v.lock() {
                            if (g.get() - meta.last_opacity).abs() > 0.01 {
                                return true;
                            }
                        }
                    }
                    false
                })
            };
        let cache_hit = used_fast_paint
            && !any_motion_active
            && self.cached_texts.is_some()
            && self.cached_svgs.is_some()
            && self.cached_images.is_some()
            && self.cached_flows.is_some();
        let (all_texts, all_svgs, all_images, flow_elements) = if cache_hit {
            (
                self.cached_texts.as_ref().unwrap().clone(),
                self.cached_svgs.as_ref().unwrap().clone(),
                self.cached_images.as_ref().unwrap().clone(),
                self.cached_flows.as_ref().unwrap().clone(),
            )
        } else {
            self.collect_render_elements_with_state(tree, Some(render_state))
        };
        let t_collect_elements = collect_start.elapsed();
        // Cache the collected elements alongside the batch on full-path
        // frames so the next fast-path frame can skip collect.
        if !used_fast_paint {
            self.cached_texts = Some(all_texts.clone());
            self.cached_svgs = Some(all_svgs.clone());
            self.cached_images = Some(all_images.clone());
            self.cached_flows = Some(flow_elements.clone());
        }

        // Separate normal content, 3D bakes and motion overlay content before dispatch.
        let mut texts = Vec::new();
        let mut fg_texts = Vec::new();
        let mut motion_subtree_texts: Vec<TextElement> = Vec::new();
        let mut layer_3d_texts: std::collections::HashMap<
            LayoutNodeId,
            (Transform3DLayerInfo, Vec<TextElement>),
        > = std::collections::HashMap::new();
        for text in all_texts {
            if let Some(ref info) = text.transform_3d_layer {
                layer_3d_texts
                    .entry(info.node_id)
                    .or_insert_with(|| (info.clone(), Vec::new()))
                    .1
                    .push(text);
            } else if text.composite_ancestor.is_some() {
                // Route promoted text through its live-opacity overlay patch, including
                // foreground content.
                motion_subtree_texts.push(text);
            } else if text.is_foreground {
                fg_texts.push(text);
            } else if text.in_motion_subtree {
                motion_subtree_texts.push(text);
            } else {
                texts.push(text);
            }
        }

        let mut svgs = Vec::new();
        let mut motion_subtree_svgs: Vec<SvgElement> = Vec::new();
        let mut layer_3d_svgs: std::collections::HashMap<LayoutNodeId, Vec<SvgElement>> =
            std::collections::HashMap::new();
        for svg in all_svgs {
            if let Some(ref info) = svg.transform_3d_layer {
                layer_3d_svgs.entry(info.node_id).or_default().push(svg);
            } else if svg.in_motion_subtree || svg.composite_ancestor.is_some() {
                // Route promoted SVGs through the same live-opacity overlay as their text.
                motion_subtree_svgs.push(svg);
            } else {
                svgs.push(svg);
            }
        }
        // Stable layer sorting preserves SVG sibling order at equal z.
        svgs.sort_by_key(|s| s.z_layer);
        motion_subtree_svgs.sort_by_key(|s| s.z_layer);

        let mut images = Vec::new();
        let mut layer_3d_images: std::collections::HashMap<LayoutNodeId, Vec<ImageElement>> =
            std::collections::HashMap::new();
        for image in all_images {
            if let Some(ref info) = image.transform_3d_layer {
                layer_3d_images.entry(info.node_id).or_default().push(image);
            } else {
                images.push(image);
            }
        }

        // Collect unique 3D layer IDs for rendering
        let layer_3d_ids: Vec<LayoutNodeId> = layer_3d_texts.keys().cloned().collect();

        // Pre-load all images into cache before rendering (both normal and 3D-layer)
        self.preload_images(&images, width as f32, height as f32);
        for layer_imgs in layer_3d_images.values() {
            self.preload_images(layer_imgs, width as f32, height as f32);
        }

        let PreparedFrameText {
            glyphs_by_layer,
            css_transformed_text_prims,
            fg_glyphs,
            fg_decorations_by_layer,
        } = self.prepare_frame_text(
            &texts,
            &fg_texts,
            &motion_subtree_texts,
            &motion_subtree_svgs,
            (width, height),
            scale_factor,
        );

        // SVGs are rendered as rasterized images (not tessellated paths) for better anti-aliasing
        // They will be rendered later via render_rasterized_svgs

        self.renderer.resize(width, height);

        // Bind the current atlas for CSS-transformed and canvas glyph primitives.
        if let (Some(atlas), Some(color_atlas)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            // The fast-path batch already contains these primitives and clip rows.
            if !used_fast_paint {
                batch.merge(css_transformed_text_prims);
            }
            self.renderer.set_glyph_atlas(atlas, color_atlas);
        }

        // Cache the emitted batch before dispatch changes its projected streams.
        tracing::trace!(
            target: "blinc_app::frame_timing",
            primitives = batch.primitives.len(),
            "cached_bg_batch_set",
        );
        self.cached_bg_batch = Some(batch.clone());
        if self.in_compositor_inner_call {
            batch.foreground_primitives.clear();
            batch.foreground_paths = Default::default();
            batch.foreground_layer_commands.clear();
            batch.foreground_dynamic_images.clear();
        }

        self.render_frame_content(
            tree,
            target,
            (width, height),
            scale_factor,
            FrameContent {
                batch: &batch,
                texts: &texts,
                images: &images,
                svgs: &svgs,
                glyphs_by_layer: &glyphs_by_layer,
                fg_glyphs: &fg_glyphs,
                fg_decorations_by_layer: &fg_decorations_by_layer,
            },
        );

        // Render 3D-layer text/SVGs/images: for each 3D layer group, render to an
        // offscreen texture and blit with the same perspective transform as the parent.
        for layer_id in &layer_3d_ids {
            if let Some((info, layer_texts)) = layer_3d_texts.get(layer_id) {
                let layer_svgs_vec = layer_3d_svgs.get(layer_id);
                let layer_images_vec = layer_3d_images.get(layer_id);
                self.render_3d_layer_elements(
                    target,
                    info,
                    layer_texts,
                    layer_svgs_vec.map(|v| v.as_slice()).unwrap_or(&[]),
                    layer_images_vec.map(|v| v.as_slice()).unwrap_or(&[]),
                    scale_factor,
                );
            }
        }

        // Render @flow shader elements on top of their SDF base
        self.has_active_flows = !flow_elements.is_empty();
        if !flow_elements.is_empty() {
            let stylesheet = tree.stylesheet();

            // Use monotonic time for smooth animation
            static START_TIME: std::sync::OnceLock<web_time::Instant> = std::sync::OnceLock::new();
            let start = START_TIME.get_or_init(web_time::Instant::now);
            let elapsed_secs = start.elapsed().as_secs_f32();

            for flow_el in &flow_elements {
                // Resolve FlowGraph: direct graph first, then stylesheet lookup
                let graph = flow_el
                    .flow_graph
                    .as_deref()
                    .or_else(|| stylesheet.and_then(|s| s.get_flow(&flow_el.flow_name)));

                if let Some(graph) = graph {
                    // Compile on first use (no-op if already cached)
                    if let Err(e) = self.renderer.flow_pipeline_cache().compile(graph) {
                        tracing::warn!("@flow '{}' compile error: {}", flow_el.flow_name, e);
                        continue;
                    }

                    let uniforms = blinc_gpu::FlowUniformData {
                        viewport_size: [width as f32, height as f32],
                        time: elapsed_secs,
                        frame_index: 0.0, // TODO: track frame counter
                        element_bounds: [flow_el.x, flow_el.y, flow_el.width, flow_el.height],
                        pointer: [
                            (self.cursor_pos[0] - flow_el.x) / flow_el.width.max(1.0),
                            (self.cursor_pos[1] - flow_el.y) / flow_el.height.max(1.0),
                        ],
                        corner_radius: flow_el.corner_radius,
                        _padding: 0.0,
                    };

                    let viewport = [flow_el.x, flow_el.y, flow_el.width, flow_el.height];
                    if !self.renderer.render_flow(
                        target,
                        &flow_el.flow_name,
                        &uniforms,
                        Some(viewport),
                    ) {
                        tracing::warn!("@flow '{}' render failed", flow_el.flow_name);
                    }
                }
            }
        }

        // Poll the device to free completed command buffers
        self.renderer.poll();

        // Draw the dynamic owner after static content. The outer compositor owns this
        // dispatch during inner cache paints.
        if !self.in_compositor_inner_call {
            if let Some(ref dynamic_batch) = self.cached_dynamic_batch {
                if !dynamic_batch.primitives.is_empty()
                    || dynamic_batch.has_paths()
                    || dynamic_batch.has_ordered_draws()
                {
                    if dynamic_batch.requires_ordered_rendering() {
                        self.renderer.render_overlay_msaa(target, dynamic_batch, 1);
                    } else {
                        self.renderer.render_overlay(target, dynamic_batch);
                    }
                    // Restore static auxiliary rows before subsequent text and SVG
                    // dispatch.
                    self.renderer.update_aux_data_for_batch(&batch);
                }
                self.renderer.render_foreground(target, dynamic_batch);
                self.renderer.update_aux_data_for_batch(&batch);
            }
        }

        // Draw motion text and SVG after their dynamic geometry.
        self.dispatch_motion_subtree_overlay_pools(tree, target);

        // Draw promoted backgrounds before separately dispatched motion text and SVG.
        if !self.in_compositor_inner_call {
            self.composite_css_layers_overlay(tree, target);
            // Replay promoted motion textures.
            self.composite_motion_layers_overlay(tree, target);
        }

        // Dispatch captured mesh output after 2D content, using each mesh's camera,
        // lighting and viewport.
        if !pending_meshes.is_empty() {
            dispatch_pending_meshes(&mut self.renderer, target, width, height, &pending_meshes);
        }
        if !pending_gpu_passes.is_empty() {
            dispatch_pending_gpu_passes(
                &mut self.renderer,
                target,
                width,
                height,
                scale_factor,
                &pending_gpu_passes,
            );
        }

        // Render overlays from RenderState
        self.render_overlays(render_state, width, height, target);

        // Render debug visualization if enabled (BLINC_DEBUG=text|layout|all)
        let debug = DebugMode::from_env();
        if debug.text {
            self.render_text_debug(target, &texts);
        }
        if debug.layout {
            let scale = tree.scale_factor();
            self.render_layout_debug(target, tree, scale);
        }
        if debug.motion {
            self.render_motion_debug(target, tree, width, height);
        }

        // Return scratch buffers for reuse on next frame
        self.return_scratch_elements(texts, svgs, images);

        // Emit periodic cache statistics.
        self.log_cache_stats();

        let t_total_p4 = p4_start.elapsed();
        let t_gpu = t_total_p4
            .saturating_sub(t_paint_walker)
            .saturating_sub(t_collect_elements);
        tracing::trace!(
            target: "blinc_app::frame_timing",
            paint_walker_us = t_paint_walker.as_micros() as u64,
            collect_elements_us = t_collect_elements.as_micros() as u64,
            gpu_us = t_gpu.as_micros() as u64,
            "p4_breakdown"
        );

        Ok(())
    }
}
