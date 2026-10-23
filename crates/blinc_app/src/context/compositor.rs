//! Compose cached content and independently dispatched frame streams.
use super::{
    CssDeltaResult, ImageElement, RenderContext, RenderTree, Result, aabb_intersects_any,
    damage_scissor_from_union, damage_union, dispatch_pending_gpu_passes, dispatch_pending_meshes,
};

impl RenderContext {
    /// Render through the static cache and independent overlay streams. Retained groups
    /// repaint together; eligible cached primitives can be patched in place.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_render_with_compositor(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target_view: &wgpu::TextureView,
        target_texture: &wgpu::Texture,
        try_fast_paint: bool,
    ) -> Result<()> {
        self.renderer.ensure_static_layer(width, height);

        // Retained callbacks share ancestor commands and cannot be refreshed
        // through the separate canvas overlay or a cached primitive splice.
        if tree.had_retained_canvas_painted() {
            self.renderer.invalidate_static_layer();
        }

        // Read fade deadlines before deciding whether the static cache can be reused.
        let now = web_time::Instant::now();
        self.image_fade_deadlines
            .retain(|_, deadline| now < *deadline);
        self.has_pending_image_fade = !self.image_fade_deadlines.is_empty();

        if self.has_pending_image_fade {
            // Refresh cached pixels while image opacity changes.
            self.renderer.invalidate_static_layer();
        }

        // Compute and commit the current animation and promotion status.
        let v2_statuses = tree.compute_animation_status();
        if tracing::enabled!(target: "blinc_app::v2_status", tracing::Level::TRACE) {
            let anim_count = v2_statuses
                .iter()
                .filter(|(_, s)| matches!(s, blinc_layout::renderer::AnimationStatus::Animating(_)))
                .count();
            let static_count = v2_statuses.len() - anim_count;
            tracing::trace!(
                target: "blinc_app::v2_status",
                anim_count,
                static_count,
                entries = ?v2_statuses,
                "v2 animation status",
            );
        }
        tree.commit_animation_status(&v2_statuses);

        // Ignore subpixel spring tails when deciding whether binding changes are visible.
        // Timeline rotations remain active while playing.
        const VISIBLE_PIXEL_EPS: f32 = 0.5;
        const VISIBLE_DEG_EPS: f32 = 0.5;
        let value_far_from_target =
            |v: &Option<blinc_animation::SharedAnimatedValue>, eps: f32| -> bool {
                v.as_ref()
                    .and_then(|s| s.lock().ok())
                    .map(|g| (g.get() - g.target()).abs() > eps)
                    .unwrap_or(false)
            };
        // Detect settled direct writes by comparing current translation with the cached
        // baseline.
        let direct_write_drift = {
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
                (cx - meta.last_translate.0).abs() > VISIBLE_PIXEL_EPS
                    || (cy - meta.last_translate.1).abs() > VISIBLE_PIXEL_EPS
            })
        };
        let bindings_animating = direct_write_drift
            || tree.motion_bindings_map().values().any(|b| {
                value_far_from_target(&b.translate_x, VISIBLE_PIXEL_EPS)
                    || value_far_from_target(&b.translate_y, VISIBLE_PIXEL_EPS)
                    || value_far_from_target(&b.scale, 0.01)
                    || value_far_from_target(&b.scale_x, 0.01)
                    || value_far_from_target(&b.scale_y, 0.01)
                    || value_far_from_target(&b.rotation, VISIBLE_DEG_EPS)
                    || value_far_from_target(&b.opacity, 0.01)
                    || b.rotation_timeline
                        .as_ref()
                        .and_then(|t| t.timeline.lock().ok())
                        .is_some_and(|g| g.is_playing())
            });
        // Read CSS activity separately from binding and structural animations.
        let css_anim_active = {
            let store = tree.css_anim_store();

            match store.lock() {
                Ok(g) => g.has_active_animations() || g.has_active_transitions(),
                Err(_) => false,
            }
        };
        if css_anim_active
            && tree
                .css_anim_paint_records()
                .values()
                .any(|meta| meta.foreground_batch)
        {
            self.renderer.invalidate_static_layer();
        }
        // Motion FSM, visual, layout and FLIP changes require the full walker. CSS may use
        // its own patch or region replay path.
        let other_animations_active = render_state.has_active_motions()
            || tree.has_active_visual_animations()
            || tree.has_active_layout_animations()
            || tree.has_active_flip_animations();
        // Use CSS-only replay when other animation systems do not require a fresh walk.
        let css_only_active = css_anim_active && !other_animations_active;
        // Diagnostic: name the predicate that's invalidating the
        // static cache. Pinpoints which animation source keeps the
        // compositor slow path active in steady state. Off unless
        // `RUST_LOG=blinc_app::cache_invalidation=trace` is set.
        if tracing::enabled!(target: "blinc_app::cache_invalidation", tracing::Level::TRACE)
            && (bindings_animating || other_animations_active)
        {
            let mb_anim = tree
                .motion_bindings_map()
                .values()
                .any(|b| b.is_any_animating());
            let fsm = render_state.has_active_motions();
            let visual = tree.has_active_visual_animations();
            let layout = tree.has_active_layout_animations();
            let flip = tree.has_active_flip_animations();
            let (css_anim, css_xition) = {
                let store = tree.css_anim_store();
                let g = store.lock();
                match g {
                    Ok(s) => (s.has_active_animations(), s.has_active_transitions()),
                    Err(_) => (false, false),
                }
            };
            tracing::trace!(
                target: "blinc_app::cache_invalidation",
                bindings = bindings_animating,
                motion_binding_anim = mb_anim,
                fsm,
                visual,
                layout,
                flip,
                css_anim,
                css_xition,
                "cache_invalidated"
            );
        }
        // Damage replay is opt-in because complex cached content may require full repaint.
        let damage_rect_enabled = std::env::var("BLINC_DAMAGE_RECT").as_deref() == Ok("1");
        let damage_rect_eligible =
            damage_rect_enabled && bindings_animating && !other_animations_active;
        if !damage_rect_eligible && other_animations_active {
            self.renderer.invalidate_static_layer();
            // Invalidate collected content before patching changes its motion baselines.
            self.cached_texts = None;
            self.cached_svgs = None;
            self.cached_images = None;
            self.cached_flows = None;
            self.cached_glyphs_by_layer = None;
            self.cached_fg_glyphs = None;
            self.cached_motion_subtree_text_prims = None;
            self.motion_subtree_text_patch.clear();
            self.cached_motion_subtree_svgs = None;
            self.cached_css_transformed_text_prims = None;
        }

        // Patch eligible CSS-only frames without rewalking the tree.
        let css_patch_eligible = css_only_active
            && !bindings_animating
            && self.cached_bg_batch.is_some()
            && self.renderer.static_layer_valid()
            && !tree.css_anim_paint_records().is_empty()
            && tree.css_active_patchable();
        if !css_patch_eligible
            && tracing::enabled!(target: "blinc_app::frame_timing", tracing::Level::TRACE)
        {
            tracing::trace!(
                target: "blinc_app::frame_timing",
                css_anim_active,
                css_only_active,
                bindings_animating,
                try_fast_paint,
                has_cached_batch = self.cached_bg_batch.is_some(),
                static_layer_valid = self.renderer.static_layer_valid(),
                css_records_present = !tree.css_anim_paint_records().is_empty(),
                "css_patch_gate_failed",
            );
        }
        if css_patch_eligible {
            let scale_factor = tree.scale_factor();
            let css_path_start = web_time::Instant::now();
            // Re-emit unsupported records when safe; otherwise repaint the full tree.
            let delta_result = self.apply_css_deltas(tree, scale_factor);
            let took_fast_path = match delta_result {
                CssDeltaResult::Patched => true,
                CssDeltaResult::PatchedWithReemits { ref reemit_nodes } => self
                    .try_reemit_and_splice_css_subtrees(
                        tree,
                        render_state,
                        reemit_nodes,
                        width,
                        height,
                    ),
                CssDeltaResult::Bail => false,
            };
            if took_fast_path {
                let batch_clone = self
                    .cached_bg_batch
                    .as_ref()
                    .expect("css_patch_eligible implies cached_bg_batch is Some")
                    .clone();

                // Try a scissored cache repaint, falling back when the batch cannot replay
                // partial damage.
                let damaged = self.last_css_damage_rects.clone();
                let damaged_ok = !damaged.is_empty()
                    && self
                        .renderer
                        .render_static_layer_damaged(&damaged, &batch_clone);

                if !damaged_ok {
                    // Repaint the whole cache when partial replay is unavailable.
                    self.renderer.render_static_layer(
                        &batch_clone,
                        [0.0, 0.0, 0.0, self.clear_alpha as f64],
                    );
                }

                // Restore collected text, SVG and images in the repainted area, or across
                // the full cache after full repaint.
                let static_view_opt = self.renderer.static_layer_view().cloned();
                if let Some(static_view) = static_view_opt {
                    if damaged_ok {
                        let union = damage_union(&damaged);
                        if let Some(scissor) = damage_scissor_from_union(union, &self.renderer) {
                            self.renderer.set_pending_scissor(scissor);
                            if let Some(glyphs_by_layer) = self.cached_glyphs_by_layer.clone() {
                                for glyphs in glyphs_by_layer.values() {
                                    let filtered = glyphs
                                        .filtered(|bounds| aabb_intersects_any(bounds, &damaged));
                                    if !filtered.is_empty() {
                                        self.render_text(&static_view, &filtered);
                                    }
                                }
                            }
                            if let Some(fg_glyphs) = self.cached_fg_glyphs.clone() {
                                let filtered = fg_glyphs
                                    .filtered(|bounds| aabb_intersects_any(bounds, &damaged));
                                if !filtered.is_empty() {
                                    self.render_text(&static_view, &filtered);
                                }
                            }
                            if let Some(svgs) = self.cached_svgs.clone() {
                                let filtered: Vec<_> = svgs
                                    .into_iter()
                                    .filter(|s| {
                                        aabb_intersects_any([s.x, s.y, s.width, s.height], &damaged)
                                    })
                                    .collect();
                                if !filtered.is_empty() {
                                    self.render_rasterized_svgs(
                                        &static_view,
                                        &filtered,
                                        scale_factor,
                                    );
                                }
                            }
                            if let Some(images) = self.cached_images.clone() {
                                let filtered_refs: Vec<&ImageElement> = images
                                    .iter()
                                    .filter(|i| {
                                        aabb_intersects_any([i.x, i.y, i.width, i.height], &damaged)
                                    })
                                    .collect();
                                if !filtered_refs.is_empty() {
                                    self.render_images_ref(&static_view, &filtered_refs);
                                }
                            }
                            self.renderer.clear_pending_scissor();
                        }
                    } else {
                        // Full re-dispatch (no scissor) — cache was
                        // fully cleared by `render_static_layer`.
                        if let Some(glyphs_by_layer) = self.cached_glyphs_by_layer.clone() {
                            for glyphs in glyphs_by_layer.values() {
                                if !glyphs.is_empty() {
                                    self.render_text(&static_view, glyphs);
                                }
                            }
                        }
                        if let Some(fg_glyphs) = self.cached_fg_glyphs.clone() {
                            if !fg_glyphs.is_empty() {
                                self.render_text(&static_view, &fg_glyphs);
                            }
                        }
                        if let Some(svgs) = self.cached_svgs.clone() {
                            if !svgs.is_empty() {
                                self.render_rasterized_svgs(&static_view, &svgs, scale_factor);
                            }
                        }
                        if let Some(images) = self.cached_images.clone() {
                            if !images.is_empty() {
                                let refs: Vec<&ImageElement> = images.iter().collect();
                                self.render_images_ref(&static_view, &refs);
                            }
                        }
                    }
                }
                // Composite canvas output before the independent dynamic overlay.
                let overlay = self.collect_canvas_overlay(tree, width, height);
                let overlay_subtree_canvases =
                    self.collect_overlay_canvas_overlay(tree, width, height);
                if !tree.dynamic_regions().is_empty() {
                    let walked =
                        self.collect_dynamic_region_primitives(tree, render_state, width, height);
                    // Keep the cached dynamic batch when filtering leaves no independently
                    // replayed regions.
                    if !walked.primitives.is_empty()
                        || walked.has_paths()
                        || walked.has_ordered_draws()
                        || walked.has_foreground_content()
                        || walked.has_foreground_layers()
                    {
                        self.cached_dynamic_batch = Some(walked);
                    }
                }
                self.rebind_glyph_atlas_for_overlay();
                self.renderer.composite_frame(
                    target_view,
                    target_texture,
                    &overlay.primitives,
                    &overlay.aux_data,
                );
                // Dispatch canvas images before foreground chrome, using their captured
                // clips.
                self.dispatch_ordered_canvas_batches(target_view, &overlay);
                if !overlay.dynamic_images.is_empty() {
                    self.renderer
                        .render_unordered_dynamic_images(target_view, &overlay.dynamic_images);
                }
                // Draw foreground above canvas content and below global overlays.
                self.render_foreground_overlay(target_view, &[&overlay]);
                self.dispatch_dynamic_batch_overlay(target_view);
                // Overlay-subtree canvases (e.g. caret blink inside a
                // focused cn::input) dispatch HERE so they land above
                // the dyn-batch SDF the popover just painted.
                self.dispatch_overlay_canvas_overlay(target_view, &overlay_subtree_canvases);
                // Composite promoted backgrounds before their separately dispatched text
                // and SVG.
                self.composite_css_layers_overlay(tree, target_view);
                // Replay promoted motion textures.
                self.composite_motion_layers_overlay(tree, target_view);
                // Draw motion text and SVG above dynamic geometry and promoted textures.
                self.dispatch_motion_subtree_overlay_pools(tree, target_view);
                if !overlay.meshes.is_empty() {
                    dispatch_pending_meshes(
                        &mut self.renderer,
                        target_view,
                        width,
                        height,
                        &overlay.meshes,
                    );
                }
                if !overlay.gpu_passes.is_empty() {
                    dispatch_pending_gpu_passes(
                        &mut self.renderer,
                        target_view,
                        width,
                        height,
                        tree.scale_factor(),
                        &overlay.gpu_passes,
                    );
                }
                // Authoritative `visible_anim_active` update —
                // walker didn't run this frame, so reset the flag
                // from current animation state. Mirrors the motion
                // damage path's end-of-frame restate.
                let has_visible_canvas =
                    !tree.canvas_paint_records().is_empty() || tree.had_retained_canvas_painted();
                let any_visible_anim = {
                    let painted_set = tree.painted_node_ids().clone();
                    let painted_stable = tree.painted_stable_ids();
                    tree.has_any_active_animation_visible(
                        render_state,
                        &painted_set,
                        &painted_stable,
                    )
                };
                tree.set_visible_anim_active(any_visible_anim || has_visible_canvas);
                tracing::trace!(
                    target: "blinc_app::frame_timing",
                    path = "css_patch",
                    damaged = damaged_ok,
                    damage_rect_count = damaged.len(),
                    gpu_us = css_path_start.elapsed().as_micros() as u64,
                    "fast_path",
                );
                return Ok(());
            }
            // apply_css_deltas returned false — out-of-scope
            // property, cache shape mismatch, or divide-by-zero.
            // Fall through to slow path.
        }

        // Fast path: cache valid AND caller is fine with reusing it.
        // Skip the entire walker / dispatch chain — just composite.
        let use_fast =
            try_fast_paint && self.renderer.static_layer_valid() && self.cached_bg_batch.is_some();

        if use_fast {
            let fast_path_start = web_time::Instant::now();
            // Patch bindings before presenting the cache. A rejected patch falls through to
            // full paint.
            let mut damage_rect_failed = false;
            // Foreground and retained static bindings need repaint even when a
            // direct write leaves the spring settled.
            let binding_repaint = self.cached_dynamic_batch.as_ref().is_some_and(|batch| {
                batch.has_foreground_content() || batch.has_foreground_layers()
            }) || tree
                .composite_bindings()
                .values()
                .any(|meta| !meta.dynamic_batch);
            let patched =
                if (bindings_animating || binding_repaint) && self.cached_dynamic_batch.is_some() {
                    let scale_factor = tree.scale_factor();
                    self.apply_binding_deltas(tree, scale_factor)
                } else {
                    true
                };
            // Handle every rejected patch, including when partial-damage replay is
            // disabled.
            if !patched {
                self.renderer.invalidate_static_layer();
                self.cached_texts = None;
                self.cached_svgs = None;
                self.cached_images = None;
                self.cached_flows = None;
                self.cached_glyphs_by_layer = None;
                self.cached_fg_glyphs = None;
                self.cached_motion_subtree_text_prims = None;
                self.motion_subtree_text_patch.clear();
                self.cached_motion_subtree_svgs = None;
                self.cached_css_transformed_text_prims = None;
                damage_rect_failed = true;
            }
            if damage_rect_eligible
                && self.cached_bg_batch.is_some()
                && patched
                && !self.last_binding_damage_rects.is_empty()
            {
                {
                    let damaged = self.last_binding_damage_rects.clone();
                    let batch_ref = self
                        .cached_bg_batch
                        .as_ref()
                        .expect("use_fast implies cached_bg_batch is Some");
                    let ok = self
                        .renderer
                        .render_static_layer_damaged(&damaged, batch_ref);
                    if !ok {
                        // Batch had content the damage-rect path
                        // can't handle (layer effects, paths, 3D
                        // viewports). Bail to full invalidation and
                        // fall through — next frame will re-walk.
                        self.renderer.invalidate_static_layer();
                        self.cached_texts = None;
                        self.cached_svgs = None;
                        self.cached_images = None;
                        self.cached_flows = None;
                        self.cached_glyphs_by_layer = None;
                        self.cached_fg_glyphs = None;
                        self.cached_motion_subtree_text_prims = None;
                        self.motion_subtree_text_patch.clear();
                        self.cached_motion_subtree_svgs = None;
                        self.cached_css_transformed_text_prims = None;
                    } else {
                        // Restore collected content inside the cleared damage region
                        // through the same scissor.
                        let static_view_opt = self.renderer.static_layer_view().cloned();
                        if let Some(static_view) = static_view_opt {
                            let union = damage_union(&damaged);
                            if let Some(scissor) = damage_scissor_from_union(union, &self.renderer)
                            {
                                let scale_factor = tree.scale_factor();
                                self.renderer.set_pending_scissor(scissor);
                                if let Some(glyphs_by_layer) = self.cached_glyphs_by_layer.clone() {
                                    for glyphs in glyphs_by_layer.values() {
                                        let filtered = glyphs.filtered(|bounds| {
                                            aabb_intersects_any(bounds, &damaged)
                                        });
                                        if !filtered.is_empty() {
                                            self.render_text(&static_view, &filtered);
                                        }
                                    }
                                }
                                if let Some(fg_glyphs) = self.cached_fg_glyphs.clone() {
                                    let filtered = fg_glyphs
                                        .filtered(|bounds| aabb_intersects_any(bounds, &damaged));
                                    if !filtered.is_empty() {
                                        self.render_text(&static_view, &filtered);
                                    }
                                }
                                // SVG bounds and damage rectangles both use physical
                                // pixels.
                                if let Some(svgs) = self.cached_svgs.clone() {
                                    let filtered: Vec<_> = svgs
                                        .into_iter()
                                        .filter(|s| {
                                            aabb_intersects_any(
                                                [s.x, s.y, s.width, s.height],
                                                &damaged,
                                            )
                                        })
                                        .collect();
                                    if !filtered.is_empty() {
                                        self.render_rasterized_svgs(
                                            &static_view,
                                            &filtered,
                                            scale_factor,
                                        );
                                    }
                                }
                                // Replay intersecting images through the damage scissor.
                                if let Some(images) = self.cached_images.clone() {
                                    let filtered_refs: Vec<&ImageElement> = images
                                        .iter()
                                        .filter(|i| {
                                            aabb_intersects_any(
                                                [i.x, i.y, i.width, i.height],
                                                &damaged,
                                            )
                                        })
                                        .collect();
                                    if !filtered_refs.is_empty() {
                                        self.render_images_ref(&static_view, &filtered_refs);
                                    }
                                }
                                self.renderer.clear_pending_scissor();
                            }
                        }
                    }
                }
            }

            // A failed patch needs a full paint on this frame.
            if damage_rect_failed {
                // Note: do NOT `return Ok(())` — proceed past the
                // `if use_fast` block to the slow path.
            } else {
                // Collect every independent canvas output channel for this frame.
                let overlay = self.collect_canvas_overlay(tree, width, height);
                let overlay_subtree_canvases =
                    self.collect_overlay_canvas_overlay(tree, width, height);
                // Refresh CSS-only motion regions without mixing their auxiliary rows or
                // paint order with canvas output.
                if css_only_active && !tree.dynamic_regions().is_empty() {
                    let walked =
                        self.collect_dynamic_region_primitives(tree, render_state, width, height);
                    // Preserve the cached dynamic batch when region filtering produces no
                    // output.
                    if !walked.primitives.is_empty()
                        || walked.has_paths()
                        || walked.has_ordered_draws()
                        || walked.has_foreground_content()
                        || walked.has_foreground_layers()
                    {
                        self.cached_dynamic_batch = Some(walked);
                    }
                }
                self.rebind_glyph_atlas_for_overlay();
                self.renderer.composite_frame(
                    target_view,
                    target_texture,
                    &overlay.primitives,
                    &overlay.aux_data,
                );
                // Canvas-emitted dynamic images (noise / texture nodes)
                // dispatched in the canvas band — before the foreground
                // overlay — and scissored to their node body. See the
                // longer note at the first fast-path site.
                self.dispatch_ordered_canvas_batches(target_view, &overlay);
                if !overlay.dynamic_images.is_empty() {
                    self.renderer
                        .render_unordered_dynamic_images(target_view, &overlay.dynamic_images);
                }
                // Draw foreground above canvas content and below global overlays.
                self.render_foreground_overlay(target_view, &[&overlay]);
                self.dispatch_dynamic_batch_overlay(target_view);
                self.dispatch_overlay_canvas_overlay(target_view, &overlay_subtree_canvases);
                // Composite promoted backgrounds before their text and SVG.
                self.composite_css_layers_overlay(tree, target_view);
                // Replay promoted motion textures.
                self.composite_motion_layers_overlay(tree, target_view);
                // Draw motion text and SVG above dynamic geometry and promoted textures.
                self.dispatch_motion_subtree_overlay_pools(tree, target_view);
                if !overlay.meshes.is_empty() {
                    dispatch_pending_meshes(
                        &mut self.renderer,
                        target_view,
                        width,
                        height,
                        &overlay.meshes,
                    );
                }
                if !overlay.gpu_passes.is_empty() {
                    dispatch_pending_gpu_passes(
                        &mut self.renderer,
                        target_view,
                        width,
                        height,
                        tree.scale_factor(),
                        &overlay.gpu_passes,
                    );
                }

                // Replace the redraw signal from visible canvases and current animation
                // activity.
                let has_visible_canvas =
                    !tree.canvas_paint_records().is_empty() || tree.had_retained_canvas_painted();
                let any_visible_anim = {
                    let painted_set = tree.painted_node_ids().clone();
                    let painted_stable = tree.painted_stable_ids();
                    tree.has_any_active_animation_visible(
                        render_state,
                        &painted_set,
                        &painted_stable,
                    )
                };
                tree.set_visible_anim_active(any_visible_anim || has_visible_canvas);

                let path_label = if bindings_animating {
                    "binding_damage"
                } else {
                    "cache_blit"
                };
                tracing::trace!(
                    target: "blinc_app::frame_timing",
                    path = path_label,
                    damage_rect_count = self.last_binding_damage_rects.len(),
                    gpu_us = fast_path_start.elapsed().as_micros() as u64,
                    "fast_path",
                );
                return Ok(());
            } // close else { ... } for !damage_rect_failed
        }

        // Repaint the static texture; independent canvases replay afterward, while retained
        // canvases stay with their groups.
        let static_view = self.renderer.static_layer_view().cloned();
        let static_view = match static_view {
            Some(v) => v,
            None => {
                // Static layer not allocated (zero-size viewport?) —
                // fall through to the non-compositor path so the
                // frame still presents.
                return self.render_tree_with_motion_opt(
                    tree,
                    render_state,
                    width,
                    height,
                    target_view,
                    None,
                    try_fast_paint,
                );
            }
        };

        tree.set_skip_canvas_drawing(true);
        // The outer compositor dispatches dynamic content once, after the static texture.
        self.in_compositor_inner_call = true;
        // Binding-only frames can patch cached geometry. Other animated content requires
        // the walker.
        let needs_walker = render_state.has_active_motions()
            || tree.has_active_visual_animations()
            || tree.has_active_layout_animations()
            || tree.has_active_flip_animations()
            // Animated scroll offsets change cached geometry without input events.
            || tree.has_animating_scroll_physics()
            || {
                let store = tree.css_anim_store();
                let guard = store.lock();
                match guard {
                    Ok(g) => g.has_active_animations() || g.has_active_transitions(),
                    Err(_) => false,
                }
            };
        let inner_try_fast = self.cached_bg_batch.is_some() && !needs_walker;
        let result = self.render_tree_with_motion_opt(
            tree,
            render_state,
            width,
            height,
            &static_view,
            None, // suppress compositor mode inside the inner call
            inner_try_fast,
        );
        self.in_compositor_inner_call = false;
        tree.set_skip_canvas_drawing(false);
        result?;

        self.renderer.mark_static_layer_valid();

        // Trace dynamic regions alongside their legacy canvas and binding records.
        if tracing::enabled!(target: "blinc_app::v2_regions", tracing::Level::TRACE) {
            let regions = tree.dynamic_regions();
            let canvas_count = regions
                .values()
                .filter(|r| matches!(r.kind, blinc_layout::renderer::DynamicKind::Canvas { .. }))
                .count();
            let motion_count = regions
                .values()
                .filter(|r| matches!(r.kind, blinc_layout::renderer::DynamicKind::MotionSubtree))
                .count();
            let css_count = regions
                .values()
                .filter(|r| {
                    matches!(
                        r.kind,
                        blinc_layout::renderer::DynamicKind::CssAnimated { .. }
                    )
                })
                .count();
            let legacy_canvas = tree.canvas_paint_records().len();
            let legacy_motion = tree.composite_bindings().len();
            tracing::trace!(
                target: "blinc_app::v2_regions",
                regions = regions.len(),
                canvas = canvas_count,
                motion = motion_count,
                css = css_count,
                legacy_canvas,
                legacy_motion,
                "v2 dynamic regions",
            );
        }

        // Collect all independent canvas output for composition with the static texture.
        let overlay = self.collect_canvas_overlay(tree, width, height);
        let overlay_subtree_canvases = self.collect_overlay_canvas_overlay(tree, width, height);
        // Keep canvas and dynamic owners in separate passes so their paint order and
        // auxiliary rows remain independent.
        self.rebind_glyph_atlas_for_overlay();
        self.renderer.composite_frame(
            target_view,
            target_texture,
            &overlay.primitives,
            &overlay.aux_data,
        );
        // Canvas-emitted dynamic images (noise / texture nodes)
        // dispatched in the canvas band — before the foreground overlay
        // — and scissored to their node body. See the longer note at
        // the first fast-path site.
        self.dispatch_ordered_canvas_batches(target_view, &overlay);
        if !overlay.dynamic_images.is_empty() {
            self.renderer
                .render_unordered_dynamic_images(target_view, &overlay.dynamic_images);
        }
        // Draw foreground above canvas content and below global overlays.
        self.render_foreground_overlay(target_view, &[&overlay]);
        self.dispatch_dynamic_batch_overlay(target_view);
        self.dispatch_overlay_canvas_overlay(target_view, &overlay_subtree_canvases);
        // Draw motion text and SVG after their dynamic geometry.
        self.dispatch_motion_subtree_overlay_pools(tree, target_view);
        // Composited CSS layers (slow path's composite site) — same
        // overlay step the fast paths do, so the texture for a
        // freshly-promoted region gets visible content on the very
        // first frame after promotion.
        self.composite_css_layers_overlay(tree, target_view);
        // Replay promoted motion textures.
        self.composite_motion_layers_overlay(tree, target_view);
        if !overlay.meshes.is_empty() {
            dispatch_pending_meshes(
                &mut self.renderer,
                target_view,
                width,
                height,
                &overlay.meshes,
            );
        }
        if !overlay.gpu_passes.is_empty() {
            dispatch_pending_gpu_passes(
                &mut self.renderer,
                target_view,
                width,
                height,
                tree.scale_factor(),
                &overlay.gpu_passes,
            );
        }

        // Keep the redraw signal current after full paint as well as cache replay.
        let has_visible_canvas =
            !tree.canvas_paint_records().is_empty() || tree.had_retained_canvas_painted();
        let any_visible_anim = {
            let painted_set = tree.painted_node_ids().clone();
            let painted_stable = tree.painted_stable_ids();
            tree.has_any_active_animation_visible(render_state, &painted_set, &painted_stable)
        };
        tree.set_visible_anim_active(any_visible_anim || has_visible_canvas);

        Ok(())
    }
}
