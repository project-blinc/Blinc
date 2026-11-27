//! Coordinate collection and ordered dispatch with optional entry/exit animation state.
use super::cached_text;
use super::composite_replay::{ALL_Z, LayerScope, MotionBand};
use super::frame_dispatch::FrameContent;
use super::frame_text::{self, PreparedFrameText};
use super::image_rendering::{blurs_in_the_frame, has_backdrop_placeholder, is_near_view};
use super::node_rows;
use super::{
    DebugMode, DrawContext, GpuPaintContext, ImageElement, LayoutNodeId, PrimitiveBatch,
    RenderContext, RenderTree, Result, SvgElement, TextElement, Transform3DLayerInfo,
    composite_replay, dispatch_pending_gpu_passes, dispatch_pending_meshes,
};
use blinc_layout::NodeMap;

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
        // Once per frame, fast paint or full: frees offscreen targets no recent frame drew.
        self.renderer.next_frame();
        self.renderer.begin_frame_submission();
        let result = self.render_tree_frame(
            tree,
            Some(render_state),
            width,
            height,
            target,
            target_texture,
            try_fast_paint,
        );
        self.renderer.end_frame_submission();
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_tree_frame(
        &mut self,
        tree: &RenderTree,
        render_state: Option<&blinc_layout::RenderState>,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
        target_texture: Option<&wgpu::Texture>,
        try_fast_paint: bool,
    ) -> Result<()> {
        if !self.overlay_frame_active {
            self.bind_tree_cache(tree, render_state.is_some());
        }
        // Consume the atlas-growth notification; frame preparation binds the current atlas
        // views.
        if !self.overlay_frame_active {
            let _ = self.text_ctx.take_atlas_grew();
        }

        // Use the static compositor when the surface texture is available. Inner cache
        // paints pass no texture.
        if let (Some(texture), Some(render_state)) = (target_texture, render_state) {
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
        tree.set_motion_promotion(self.promote_motion && !self.renderer.node_table_enabled());
        tree.set_scroll_rows(self.rows_mode());
        let v2_statuses = tree.compute_animation_status();
        tree.commit_animation_status(&v2_statuses);
        if !self.in_compositor_inner_call && !self.overlay_frame_active {
            self.refresh_css_cache_ownership(tree);
            self.apply_retained_background_colors(tree);
        }

        // Direct frames have no CSS cache patch pass; active CSS needs a full walk.
        // A frame that reuses the cached batches also reuses the collected content, so it needs
        // that content to be the last full paint's, not cleared since.
        let canvas_output = (try_fast_paint
            && self.collected_valid
            && self.cached_glyphs_by_layer.is_some()
            && self.cached_fg_glyphs.is_some()
            && self.cached_css_transformed_text_prims.is_some()
            && !self.images_need_repaint
            && !tree.css_active_off_rows())
        .then(|| self.redraw_canvas_output(tree, width, height))
        .flatten()
        .filter(|_| self.apply_binding_deltas(tree, scale_factor));
        let used_fast_paint = canvas_output.is_some();
        if !used_fast_paint {
            self.mark_pending_backdrops(tree);
        }

        // Create a single paint context for all layers with text rendering support
        let rows_mode = self.rows_mode();
        let mut ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);
        ctx.enable_deferred_text();
        ctx.set_moving_groups_dynamic(rows_mode);
        ctx.enable_deferred_svg();
        ctx.enable_deferred_image();
        ctx.enable_glass_layers();
        ctx.set_gpu_pass_scale_factor(scale_factor as f64);

        // A successful patch already contains this frame's geometry.
        if !used_fast_paint {
            if let Some(state) = render_state {
                tree.render_with_motion(&mut ctx, state);
            } else {
                tree.render_snapshot(&mut ctx);
            }
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
        let mut pending_meshes = ctx.take_pending_meshes();
        // Dispatch callback GPU passes alongside captured meshes after 2D content.
        let mut pending_gpu_passes = ctx.take_pending_gpu_passes();
        if let Some(output) = canvas_output {
            pending_meshes.extend(output.meshes);
            pending_gpu_passes.extend(output.gpu_passes);
        }

        // Collect text, SVG, images and flows, reusing compatible cached element lists.
        let collect_start = web_time::Instant::now();
        // Refresh collected positions for active motion and direct-write drift. Pure
        // timeline rotation can keep unrelated collected content cached.
        let any_motion_active = render_state.is_some_and(|state| state.has_active_motions())
            || tree.has_active_visual_animations()
            || tree.has_active_layout_animations()
            || tree
                .motion_bindings_map()
                .values()
                .any(|b| b.is_any_position_animating())
            || {
                let bindings_table = tree.motion_bindings_map();
                tree.composite_bindings().iter().any(|(node, meta)| {
                    if let Some((x, y)) = tree.property_row_translation(*node) {
                        return (x - meta.last_translate.0).abs() > f32::EPSILON
                            || (y - meta.last_translate.1).abs() > f32::EPSILON;
                    }
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
                    if let Some(ref v) = b.opacity
                        && let Ok(g) = v.lock()
                        && (g.get() - meta.last_opacity).abs() > 0.01
                    {
                        return true;
                    }
                    false
                })
            };
        let cache_hit = used_fast_paint
            && !any_motion_active
            && self.cached_texts.is_some()
            && self.cached_svgs.is_some()
            && self.cached_images.is_some();
        let (all_texts, all_svgs, all_images) = if cache_hit {
            (
                self.cached_texts.as_ref().unwrap().clone(),
                self.cached_svgs.as_ref().unwrap().clone(),
                self.cached_images.as_ref().unwrap().clone(),
            )
        } else {
            self.collect_render_elements_with_state(tree, render_state)
        };
        let t_collect_elements = collect_start.elapsed();
        // Cache the collected elements alongside the batch on full-path
        // frames so the next fast-path frame can skip collect.
        if !used_fast_paint {
            self.cached_texts = Some(all_texts.clone());
            self.cached_svgs = Some(all_svgs.clone());
            self.cached_images = Some(all_images.clone());
        }

        // Separate normal content, 3D bakes and motion overlay content before dispatch.
        let mut texts = Vec::new();
        let mut fg_texts = Vec::new();
        let mut motion_subtree_texts: Vec<TextElement> = Vec::new();
        // The 3D layers that hold any text, SVG or image, and their content.
        let mut layers_3d: NodeMap<Transform3DLayerInfo> = std::collections::HashMap::default();
        let mut layer_3d_texts: NodeMap<Vec<TextElement>> = std::collections::HashMap::default();
        let mut captured = std::collections::HashMap::new();
        if !used_fast_paint {
            self.captured_runs.clear();
        }
        let captured_nodes: std::collections::HashSet<_> = batch
            .cross_stream_groups
            .text_nodes
            .iter()
            .chain(
                self.cached_dynamic_batch
                    .iter()
                    .flat_map(|b| &b.cross_stream_groups.text_nodes),
            )
            .copied()
            .collect();
        for text in all_texts {
            use slotmap::Key as _;
            let key = text.node.data().as_ffi();
            if captured_nodes.contains(&key) {
                if !used_fast_paint {
                    // Glyphs inside a group carry the ordinal of the node that moves them. With the
                    // node table they are also sized and placed as motion text is, and kept
                    // sharp as their node moves and settles.
                    let ordinal = node_rows::ordinal_of(tree, text.node);
                    let mut run_id = 0u32;
                    let table = self.rows_mode();
                    let mut prepared = if table && ordinal != 0 {
                        let (scale, moving) = frame_text::raster_scale(tree, text.node);
                        let shown = scale.max(frame_text::affine_scale(text.css_affine));
                        let k = frame_text::raster_size(shown, text.font_size, moving);
                        if table {
                            run_id = self.captured_runs.len() as u32 + 1;
                            self.captured_runs.push(cached_text::CapturedRun {
                                id: run_id,
                                element: text.clone(),
                                k,
                                snapped: !moving,
                                baked: None,
                            });
                        }
                        self.prepare_captured_text_at(
                            &text,
                            (width, height),
                            scale_factor,
                            k,
                            Some(!moving || frame_text::scaled(shown)),
                        )
                    } else {
                        // Not moved by the node table: laid out at the base size and rasterized at
                        // the size its card shows it, so a scaled card is as sharp faded or not.
                        let shown = frame_text::affine_scale(text.css_affine);
                        let moving = frame_text::css_transforming(tree, text.node)
                            || frame_text::chain_moving(tree, text.node);
                        let k = frame_text::raster_size(shown, text.font_size, moving);
                        self.prepare_captured_text_at(
                            &text,
                            (width, height),
                            scale_factor,
                            k,
                            text.css_affine
                                .map(|_| !moving || frame_text::scaled(shown)),
                        )
                    };
                    for p in &mut prepared.primitives {
                        p.set_node_ordinal(ordinal);
                        p.node_slot[3] = run_id as f32;
                    }
                    // A styled node can contribute several span and line runs.
                    match captured.entry(key) {
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(prepared);
                        }
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            entry.get_mut().merge(prepared);
                        }
                    }
                }
                continue;
            }
            if let Some(ref info) = text.transform_3d_layer {
                layers_3d
                    .entry(info.node_id)
                    .or_insert_with(|| info.clone());
                layer_3d_texts.entry(info.node_id).or_default().push(text);
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
        let mut layer_3d_svgs: NodeMap<Vec<SvgElement>> = std::collections::HashMap::default();
        let captured_svgs: std::collections::HashSet<_> = batch
            .cross_stream_groups
            .svg_nodes
            .iter()
            .chain(
                self.cached_dynamic_batch
                    .iter()
                    .flat_map(|b| &b.cross_stream_groups.svg_nodes),
            )
            .copied()
            .collect();
        if !used_fast_paint && !captured_svgs.is_empty() {
            self.svg_atlas.begin_frame(&self.device);
        }
        for svg in all_svgs {
            use slotmap::Key as _;
            let key = svg.node.data().as_ffi();
            if captured_svgs.contains(&key) {
                if !used_fast_paint {
                    captured.insert(
                        key,
                        self.prepare_captured_svg(
                            &svg,
                            (width, height),
                            scale_factor,
                            super::node_rows::ordinal_of(tree, svg.node),
                        ),
                    );
                }
                continue;
            }
            if let Some(ref info) = svg.transform_3d_layer {
                layers_3d
                    .entry(info.node_id)
                    .or_insert_with(|| info.clone());
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
        let mut motion_subtree_images: Vec<ImageElement> = Vec::new();
        let mut layer_3d_images: NodeMap<Vec<ImageElement>> = std::collections::HashMap::default();
        self.preload_images(&all_images, width as f32, height as f32);
        let captured_images: std::collections::HashSet<_> = batch
            .cross_stream_groups
            .image_nodes
            .iter()
            .chain(
                self.cached_dynamic_batch
                    .iter()
                    .flat_map(|b| &b.cross_stream_groups.image_nodes),
            )
            .copied()
            .collect();
        self.images_need_repaint = false;
        let mut skeleton_shown = false;
        let mut glass_placeholders = Vec::new();
        for image in all_images {
            use slotmap::Key as _;
            let key = image.node.data().as_ffi();
            let missing = !self.image_cache.contains(&image.source);
            // A skeleton moves, and a glass placeholder is in the cached batch, until the
            // image arrives.
            if missing
                && !self.image_failures.gave_up(&image.source)
                && is_near_view(&image, (width as f32, height as f32), 0.0)
            {
                skeleton_shown |= image.placeholder_type == 3;
                self.images_need_repaint |=
                    image.placeholder_type == 3 || has_backdrop_placeholder(&image);
            }
            if captured_images.contains(&key) {
                let fading = self
                    .image_fade_deadlines
                    .get(&image.source)
                    .is_some_and(|deadline| web_time::Instant::now() < *deadline);
                self.images_need_repaint |= missing || fading;
                if !used_fast_paint {
                    captured.insert(
                        key,
                        self.prepare_captured_image(
                            &image,
                            (width, height),
                            scale_factor,
                            super::node_rows::ordinal_of(tree, image.node),
                        ),
                    );
                }
                continue;
            }

            if missing && !used_fast_paint && image.opacity > 0.0 && blurs_in_the_frame(&image) {
                glass_placeholders.push(self.paint_image(
                    &image,
                    image.opacity,
                    (width, height),
                    scale_factor,
                    0,
                    true,
                ));
            }

            if let Some(ref info) = image.transform_3d_layer {
                layers_3d
                    .entry(info.node_id)
                    .or_insert_with(|| info.clone());
                layer_3d_images.entry(info.node_id).or_default().push(image);
            } else if image.in_motion_subtree {
                motion_subtree_images.push(image);
            } else {
                images.push(image);
            }
        }
        // Images inside motion containers are drawn after the dynamic batch their card's
        // background is in, so they sit above it.
        motion_subtree_images.sort_by_key(|i| i.z_index);
        if !used_fast_paint {
            self.cached_motion_subtree_images =
                (!motion_subtree_images.is_empty()).then_some(motion_subtree_images);
            self.collected_valid = true;
        }

        if !used_fast_paint {
            batch.resolve_deferred_content(&mut captured);
            self.preload_mask_images(&batch);
            if let Some(mut dynamic) = self.cached_dynamic_batch.take() {
                dynamic.resolve_deferred_content(&mut captured);
                self.preload_mask_images(&dynamic);
                self.cached_dynamic_batch = Some(dynamic);
            }
            // Merging closes the batch's streams, so it follows the deferred content.
            for glass in glass_placeholders {
                batch.merge(glass);
            }
        }
        // Collect unique 3D layer IDs for rendering
        let PreparedFrameText {
            glyphs_by_layer,
            css_transformed_text_prims,
            fg_glyphs,
            fg_decorations_by_layer,
        } = if used_fast_paint {
            // The row update already refined motion glyphs. Preparing them again would
            // discard their row ownership and make the following frame repaint.
            PreparedFrameText {
                glyphs_by_layer: self.cached_glyphs_by_layer.as_ref().unwrap().clone(),
                css_transformed_text_prims: self
                    .cached_css_transformed_text_prims
                    .as_ref()
                    .unwrap()
                    .clone(),
                fg_glyphs: self.cached_fg_glyphs.as_ref().unwrap().clone(),
                fg_decorations_by_layer: super::generate_text_decoration_primitives_by_layer(
                    &fg_texts,
                ),
            }
        } else {
            // Text under a scaling ancestor is rasterized at its displayed size.
            let raster_scales = motion_subtree_texts
                .iter()
                .map(|t| (t.node, frame_text::raster_scale(tree, t.node)))
                .collect();
            self.prepare_frame_text(
                &texts,
                &fg_texts,
                &motion_subtree_texts,
                &motion_subtree_svgs,
                &raster_scales,
                (width, height),
                scale_factor,
            )
        };

        // The batch and the glyphs of motion subtrees were just painted at one pose: the node
        // table takes both, and later frames only write its rows.
        if !used_fast_paint {
            let _ = self.try_node_rows(tree, scale_factor, true);
        }

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
        // The foreground goes above the motion content drawn after the dynamic batch, so it is
        // drawn there: by the compositor outside its cache paint, here after the dynamic batch.
        // Grouped glass reads a backdrop only the frame content prepares, and stays with it. An
        // overlay tree's content is all above its static foreground, which stays first.
        let defer_foreground = !self.in_compositor_inner_call
            && !self.overlay_frame_active
            && !batch.foreground_batch().has_grouped_glass();
        if self.in_compositor_inner_call || defer_foreground {
            batch.foreground_primitives.clear();
            batch.foreground_paths = Default::default();
            batch.foreground_layer_commands.clear();
            batch.foreground_dynamic_images.clear();
        }
        let no_glyphs = cached_text::TextBatch::default();
        let no_decorations = std::collections::HashMap::new();

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
                fg_glyphs: if defer_foreground {
                    &no_glyphs
                } else {
                    &fg_glyphs
                },
                fg_decorations_by_layer: if defer_foreground {
                    &no_decorations
                } else {
                    &fg_decorations_by_layer
                },
            },
        );

        // Render 3D-layer text/SVGs/images: for each 3D layer group, render to an
        // offscreen texture and blit with the same perspective transform as the parent.
        for (layer_id, info) in &layers_3d {
            self.render_3d_layer_elements(
                target,
                info,
                layer_3d_texts.get(layer_id).map_or(&[], Vec::as_slice),
                layer_3d_svgs.get(layer_id).map_or(&[], Vec::as_slice),
                layer_3d_images.get(layer_id).map_or(&[], Vec::as_slice),
                scale_factor,
            );
        }

        self.has_active_flows = skeleton_shown;

        // Poll the device to free completed command buffers
        self.renderer.poll();

        // Draw the dynamic owner after static content. The outer compositor owns this
        // dispatch during inner cache paints. Layers of the main tree sit below the overlay
        // content the dynamic batch holds.
        if !self.in_compositor_inner_call {
            self.composite_css_layers_overlay(tree, target, LayerScope::Main);
            self.composite_motion_layers_overlay(tree, target, LayerScope::Main);
            self.dispatch_dynamic_batch_and_motion(tree, target, |this| {
                if defer_foreground {
                    this.render_foreground_overlay(target, &[]);
                    for &z in fg_decorations_by_layer.keys() {
                        this.render_text_decorations_for_layer(target, &fg_decorations_by_layer, z);
                    }
                }
            });
            // Restore static auxiliary rows before subsequent text and SVG dispatch.
            self.renderer.update_aux_data_for_batch(&batch);
            self.draw_scroll_row_scrollbars(tree, target);
        }

        // Draw motion text and SVG after their dynamic geometry. Inside the compositor's cache
        // paint the target is the static layer, and the compositor draws them again on top of it
        // each frame. Where motion is not baked to textures (the node table, or vectors) that copy
        // would stay at the pose of this paint while the other moves.
        let motion_is_vectors = self.rows_mode();
        if self.in_compositor_inner_call && !motion_is_vectors {
            self.dispatch_motion_subtree_overlay_pools(tree, target, MotionBand::Below, ALL_Z);
        }
        if !(self.in_compositor_inner_call && motion_is_vectors) {
            self.dispatch_motion_subtree_overlay_pools(tree, target, MotionBand::Foreground, ALL_Z);
        }

        // Draw promoted backgrounds before separately dispatched motion text and SVG.
        if !self.in_compositor_inner_call {
            self.composite_css_layers_overlay(tree, target, LayerScope::Overlay);
            // Replay promoted motion textures.
            self.composite_motion_layers_overlay(tree, target, LayerScope::Overlay);
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
        if let Some(state) = render_state.filter(|_| !self.overlay_frame_active) {
            self.render_overlays(state, width, height, target);
        }

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
            used_fast_paint,
            "p4_breakdown"
        );

        Ok(())
    }
}
