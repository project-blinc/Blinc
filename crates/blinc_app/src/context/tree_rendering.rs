//! Render a layout tree and its text through the native pipelines.
use super::{
    CachedTexture, Color, GenericFont, GpuGenericFont, GpuGlyph, GpuPaintContext, GpuPrimitive,
    PrimitiveBatch, RenderContext, RenderLayer, RenderState, RenderTree, Result, TextAlign,
    TextAlignment, TextAnchor, TextElement, TextVerticalAlign, text_clip,
};

impl RenderContext {
    /// Render a layout tree, including its text, images, SVGs and effect layers.
    pub fn render_tree(
        &mut self,
        tree: &RenderTree,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        // Get scale factor for HiDPI rendering
        let scale_factor = tree.scale_factor();

        // Create paint contexts for each layer with text rendering support
        let mut bg_ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);

        // Render layout layers (background and glass go to bg_ctx)
        tree.render_to_layer(&mut bg_ctx, RenderLayer::Background);
        tree.render_to_layer(&mut bg_ctx, RenderLayer::Glass);

        // Take the batch from bg_ctx before we can reuse text_ctx for fg_ctx
        let mut bg_batch = bg_ctx.take_batch();

        // Create foreground context with text rendering support
        let mut fg_ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);
        tree.render_to_layer(&mut fg_ctx, RenderLayer::Foreground);

        // Take the batch from fg_ctx before reusing text_ctx for text elements
        let mut fg_batch = fg_ctx.take_batch();
        // Explicit canvas foreground stays ordered after the separate layer passes.
        let foreground = [bg_batch.take_foreground(), fg_batch.take_foreground()];

        // Collect text, SVG, image, and flow elements
        let (texts, svgs, images, _flows) = self.collect_render_elements(tree);

        // Pre-load all images into cache before rendering
        self.preload_images(&images, width as f32, height as f32);

        // Prepare text glyphs
        let mut text_batch = PrimitiveBatch::new();
        let mut css_text = PrimitiveBatch::new();
        for text in &texts {
            // Convert layout TextAlign to GPU TextAlignment
            let alignment = match text.align {
                TextAlign::Left => TextAlignment::Left,
                TextAlign::Center => TextAlignment::Center,
                TextAlign::Right => TextAlignment::Right,
            };

            // Use font metrics for baseline alignment and the layout box for centered or
            // top-aligned text.
            let (anchor, y_pos, use_layout_height) = match text.v_align {
                TextVerticalAlign::Center => {
                    (TextAnchor::Center, text.y + text.height / 2.0, false)
                }
                TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                TextVerticalAlign::Baseline => {
                    // Use the actual font ascender for baseline positioning.
                    // This ensures each font aligns by its true baseline.
                    let baseline_y = text.y + text.half_leading + text.ascender;
                    (TextAnchor::Baseline, baseline_y, false)
                }
            };

            // Determine wrap width: use clip bounds if available (parent constraint),
            // otherwise use the text element's own layout width
            let wrap_width = if text.wrap {
                if let Some(clip) = text.clip_bounds {
                    // clip[2] is the clip width - use it if smaller than text width
                    clip[2].min(text.width)
                } else {
                    text.width
                }
            } else {
                text.width
            };

            // Convert font family to GPU types
            let font_name = text.font_family.name.as_deref();
            let generic = to_gpu_generic_font(text.font_family.generic);
            let font_weight = text.weight.weight();

            // Only pass layout_height when we want centering within the box
            let layout_height = if use_layout_height {
                Some(text.height)
            } else {
                None
            };

            match self.text_ctx.prepare_text_with_style(
                &text.content,
                text.x,
                y_pos,
                text.font_size,
                text.color,
                anchor,
                alignment,
                Some(wrap_width),
                text.wrap,
                font_name,
                generic,
                font_weight,
                text.italic,
                layout_height,
                text.letter_spacing,
            ) {
                Ok(glyphs) => {
                    tracing::trace!(
                        "Prepared {} glyphs for text '{}' (font={:?}, generic={:?})",
                        glyphs.len(),
                        text.content,
                        font_name,
                        generic
                    );
                    let mut owned = text_clip::prepare_glyphs(text, glyphs, (width, height));

                    if let Some(affine) = text.css_affine {
                        // CSS-transformed text: convert to SDF primitives with local_affine
                        let [a, b, c, d, tx, ty] = affine;
                        let tx_scaled = tx * scale_factor;
                        let ty_scaled = ty * scale_factor;
                        for glyph in &owned.glyphs {
                            let gc_x = glyph.bounds[0] + glyph.bounds[2] / 2.0;
                            let gc_y = glyph.bounds[1] + glyph.bounds[3] / 2.0;
                            let new_gc_x = a * gc_x + c * gc_y + tx_scaled;
                            let new_gc_y = b * gc_x + d * gc_y + ty_scaled;
                            let mut prim = GpuPrimitive::from_glyph(glyph);
                            prim.bounds = [
                                new_gc_x - glyph.bounds[2] / 2.0,
                                new_gc_y - glyph.bounds[3] / 2.0,
                                glyph.bounds[2],
                                glyph.bounds[3],
                            ];
                            prim.local_affine = [a, b, c, d];
                            owned.primitives.push(prim);
                        }
                        owned.glyphs.clear();
                        css_text.merge(owned);
                    } else {
                        // Snap glyph origins to physical pixels so atlas sampling stays
                        // sharp.
                        for glyph in &mut owned.glyphs {
                            glyph.bounds[0] = glyph.bounds[0].round();
                            glyph.bounds[1] = glyph.bounds[1].round();
                        }
                        text_batch.merge(owned);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to prepare text '{}': {:?}", text.content, e);
                }
            }
        }

        tracing::trace!(
            "Text rendering: {} texts collected, {} total glyphs prepared",
            texts.len(),
            text_batch.glyphs.len()
        );

        // Prepared text and its headers travel with the foreground batch.
        fg_batch.merge(text_batch);

        // SVGs are rendered as rasterized images (not tessellated paths) for better anti-aliasing
        // They will be rendered later via render_rasterized_svgs

        self.renderer.resize(width, height);

        // Bind the current atlas for both CSS-transformed and canvas glyph primitives.
        if let (Some(atlas), Some(color_atlas)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            bg_batch.merge(css_text);
            self.renderer.set_glyph_atlas(atlas, color_atlas);
        }

        let has_glass = bg_batch.glass_count() > 0;

        // Only allocate glass textures when glass is actually used
        if has_glass {
            self.ensure_glass_textures(width, height);
        }
        let use_msaa_overlay = self.sample_count > 1;

        // Background layer uses SDF rendering (shader-based AA, no MSAA needed)
        // Foreground layer (SVGs as tessellated paths) uses MSAA for smooth edges

        if has_glass {
            // Split images by layer: background images go behind glass (get blurred),
            // glass/foreground images render on top of glass (not blurred)
            let (bg_images, fg_images): (Vec<_>, Vec<_>) = images
                .iter()
                .partition(|img| img.layer == RenderLayer::Background);

            // Pre-render background images to both backdrop and target so glass can blur them
            let has_bg_images = !bg_images.is_empty();
            if has_bg_images {
                // Take backdrop temporarily to avoid borrow conflict with render_images_ref(&mut self)
                let backdrop_tex = self.backdrop_texture.take().unwrap();
                self.renderer
                    .clear_target(&backdrop_tex.view, wgpu::Color::TRANSPARENT);
                self.renderer.clear_target(
                    target,
                    wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: self.clear_alpha as f64,
                    },
                );
                self.render_images_ref(&backdrop_tex.view, &bg_images);
                self.render_images_ref(target, &bg_images);
                self.backdrop_texture = Some(backdrop_tex);
            }

            // Glass path - batched rendering for reduced command buffer overhead:
            // Steps 1-3 are batched into a single encoder submission
            {
                let backdrop = self.backdrop_texture.as_ref().unwrap();
                self.renderer.render_glass_frame(
                    target,
                    &backdrop.view,
                    (backdrop.width, backdrop.height),
                    &bg_batch,
                    has_bg_images,
                );
            }

            // Dispatch paths with MSAA when configured, otherwise through the single-sample
            // path.
            if bg_batch.has_paths() && !bg_batch.has_ordered_draws() {
                if use_msaa_overlay {
                    self.renderer
                        .render_paths_overlay_msaa(target, &bg_batch, self.sample_count);
                } else {
                    self.renderer.render_paths_overlay(target, &bg_batch);
                }
            }

            // Render remaining bg images to target (only if not already pre-rendered)
            if !has_bg_images {
                self.render_images_ref(target, &bg_images);
            }

            // Step 5: Render glass/foreground-layer images (on top of glass, NOT blurred)
            self.render_images_ref(target, &fg_images);

            // Step 5b: Render dynamic RGBA images (video frames, camera preview)
            if !bg_batch.dynamic_images.is_empty() {
                self.renderer
                    .render_unordered_dynamic_images(target, &bg_batch.dynamic_images);
            }
            if !fg_batch.dynamic_images.is_empty() {
                self.renderer
                    .render_unordered_dynamic_images(target, &fg_batch.dynamic_images);
            }

            // Step 6: Render foreground and text
            // Use batch-based rendering when layer effects are present
            let has_layer_effects = fg_batch.requires_ordered_rendering();
            if has_layer_effects {
                // Layer effects require batch-based rendering to process layer commands
                fg_batch.primitives.extend(
                    fg_batch
                        .glyphs
                        .drain(..)
                        .map(|glyph| GpuPrimitive::from_glyph(&glyph)),
                );
                if !fg_batch.is_empty() {
                    // Pre-load any mask images referenced by layer effects
                    self.preload_mask_images(&fg_batch);
                    self.renderer.render_overlay(target, &fg_batch);
                }
                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            } else if self.renderer.unified_text_rendering() {
                // The foreground batch owns both converted glyphs and their aux rows.
                let unified_primitives = fg_batch.get_unified_foreground_primitives();
                if !unified_primitives.is_empty() {
                    self.renderer.update_aux_data_for_batch(&fg_batch);
                    self.render_unified(target, &unified_primitives);
                }

                // Render paths (not included in unified primitives).
                // MSAA when available; non-MSAA dispatch when not so
                // web (sample_count = 1) still gets fill_path content.
                if fg_batch.has_paths() && !fg_batch.has_ordered_draws() {
                    if use_msaa_overlay {
                        self.renderer.render_paths_overlay_msaa(
                            target,
                            &fg_batch,
                            self.sample_count,
                        );
                    } else {
                        self.renderer.render_paths_overlay(target, &fg_batch);
                    }
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            } else {
                // Legacy rendering: separate foreground and text passes
                if !fg_batch.is_empty() {
                    if use_msaa_overlay {
                        self.renderer
                            .render_overlay_msaa(target, &fg_batch, self.sample_count);
                    } else {
                        self.renderer.render_overlay(target, &fg_batch);
                    }
                }

                // Step 7: Render text
                if !fg_batch.glyphs.is_empty() {
                    self.render_owned_text(target, &fg_batch);
                }

                // Paths in the legacy branch also need a 1×-sample
                // fallback dispatch — same reason as the unified
                // branch above.
                if fg_batch.has_paths() && !fg_batch.has_ordered_draws() {
                    if use_msaa_overlay {
                        self.renderer.render_paths_overlay_msaa(
                            target,
                            &fg_batch,
                            self.sample_count,
                        );
                    } else {
                        self.renderer.render_paths_overlay(target, &fg_batch);
                    }
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            }

            // Step 8: Render text decorations (strikethrough, underline)
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);
            for primitives in decorations_by_layer.values() {
                if !primitives.is_empty() {
                    self.render_unified(target, primitives);
                }
            }
        } else {
            // Simple path (no glass):
            // Background uses SDF rendering (no MSAA needed)
            // Foreground uses MSAA for smooth SVG edges

            // Render background directly to target. Alpha comes from
            // `clear_alpha` so transparent windows get a fully clear
            // surface (0.0) while opaque windows keep the historical
            // opaque black (1.0) clear.
            self.renderer.render_with_clear(
                target,
                &bg_batch,
                [0.0, 0.0, 0.0, self.clear_alpha as f64],
            );

            // Render background paths. Same MSAA/non-MSAA split as the
            // glass branch — 1×-sample targets still have to dispatch.
            if bg_batch.has_paths() && !bg_batch.has_ordered_draws() {
                if use_msaa_overlay {
                    self.renderer
                        .render_paths_overlay_msaa(target, &bg_batch, self.sample_count);
                } else {
                    self.renderer.render_paths_overlay(target, &bg_batch);
                }
            }

            // Render images after background primitives
            self.render_images(target, &images, width as f32, height as f32, scale_factor);

            // Render foreground and text
            // Use batch-based rendering when layer effects are present to preserve
            // layer commands for effect processing
            let has_layer_effects = fg_batch.requires_ordered_rendering();
            if has_layer_effects {
                // Layer effects require batch-based rendering to process layer commands
                // First convert glyphs to primitives so they're included in the batch
                fg_batch.primitives.extend(
                    fg_batch
                        .glyphs
                        .drain(..)
                        .map(|glyph| GpuPrimitive::from_glyph(&glyph)),
                );

                // Use render_overlay which supports layer effect processing
                if !fg_batch.is_empty() {
                    self.renderer.render_overlay(target, &fg_batch);
                }
                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            } else if self.renderer.unified_text_rendering() {
                // Unified glyphs retain the foreground batch's auxiliary offsets.
                let unified_primitives = fg_batch.get_unified_foreground_primitives();
                if !unified_primitives.is_empty() {
                    self.renderer.update_aux_data_for_batch(&fg_batch);
                    self.render_unified(target, &unified_primitives);
                }

                // Render paths (not included in unified primitives).
                // 1×-sample fallback dispatch so web still emits.
                if fg_batch.has_paths() && !fg_batch.has_ordered_draws() {
                    if use_msaa_overlay {
                        self.renderer.render_paths_overlay_msaa(
                            target,
                            &fg_batch,
                            self.sample_count,
                        );
                    } else {
                        self.renderer.render_paths_overlay(target, &fg_batch);
                    }
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            } else {
                // Legacy rendering: separate foreground and text passes
                if !fg_batch.is_empty() {
                    if use_msaa_overlay {
                        self.renderer
                            .render_overlay_msaa(target, &fg_batch, self.sample_count);
                    } else {
                        self.renderer.render_overlay(target, &fg_batch);
                    }
                }

                // Render text
                if !fg_batch.glyphs.is_empty() {
                    self.render_owned_text(target, &fg_batch);
                }

                // Paths in the legacy branch — 1×-sample fallback so
                // web emits fill_path / stroke_path content.
                if fg_batch.has_paths() && !fg_batch.has_ordered_draws() {
                    if use_msaa_overlay {
                        self.renderer.render_paths_overlay_msaa(
                            target,
                            &fg_batch,
                            self.sample_count,
                        );
                    } else {
                        self.renderer.render_paths_overlay(target, &fg_batch);
                    }
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            }

            // Render text decorations (strikethrough, underline)
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);
            for primitives in decorations_by_layer.values() {
                if !primitives.is_empty() {
                    self.render_unified(target, primitives);
                }
            }
        }

        self.rebind_glyph_atlas_for_overlay();
        for batch in &foreground {
            if !batch.is_empty() {
                self.renderer.render_overlay_msaa(target, batch, 1);
            }
        }

        // Return scratch buffers for reuse on next frame
        self.return_scratch_elements(texts, svgs, images);

        // Poll the device to free completed command buffers and prevent memory accumulation
        self.renderer.poll();

        Ok(())
    }

    /// Ensure glass-related textures exist and are the right size.
    /// Only called when glass elements are present in the scene.
    ///
    /// We use a single texture for both rendering and sampling (backdrop_texture).
    /// The texture is rendered at half resolution to save memory (blur doesn't need full res).
    pub(super) fn ensure_glass_textures(&mut self, width: u32, height: u32) {
        // Use the same texture format as the renderer's pipelines
        let format = self.renderer.texture_format();

        // Use half resolution for glass backdrop - blur effect doesn't need full resolution
        // This saves 75% of texture memory (e.g., 2.5MB -> 0.6MB for 900x700 window)
        let backdrop_width = (width / 2).max(1);
        let backdrop_height = (height / 2).max(1);

        let needs_backdrop = self
            .backdrop_texture
            .as_ref()
            .map(|t| t.width != backdrop_width || t.height != backdrop_height)
            .unwrap_or(true);

        if needs_backdrop {
            // Single texture that can be both rendered to AND sampled from
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Glass Backdrop"),
                size: wgpu::Extent3d {
                    width: backdrop_width,
                    height: backdrop_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.backdrop_texture = Some(CachedTexture {
                texture,
                view,
                width: backdrop_width,
                height: backdrop_height,
            });
        }
    }

    /// Render prepared glyphs with the configured sample count.
    pub(super) fn render_legacy_text(&mut self, target: &wgpu::TextureView, glyphs: &[GpuGlyph]) {
        if let (Some(atlas_view), Some(color_atlas_view)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            self.renderer.render_text(
                target,
                glyphs,
                atlas_view,
                color_atlas_view,
                self.text_ctx.sampler(),
            );
        }
    }

    /// Render text and shape primitives through the shared SDF pipelines.
    pub(super) fn render_unified(
        &mut self,
        target: &wgpu::TextureView,
        primitives: &[GpuPrimitive],
    ) {
        if primitives.is_empty() {
            return;
        }

        // Use MSAA for the unified primitive stream, including tessellated fills and
        // atlas-backed glyphs.
        if self.sample_count > 1 {
            self.renderer
                .render_primitives_overlay_msaa(target, primitives, self.sample_count);
            return;
        }

        if let (Some(atlas_view), Some(color_atlas_view)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            self.renderer.render_primitives_overlay_with_glyphs(
                target,
                primitives,
                atlas_view,
                color_atlas_view,
            );
        } else {
            // No atlas available — fall back to plain primitive
            // rendering. Text quads will sample placeholder pixels
            // and render invisibly, but at least non-text primitives
            // still render.
            self.renderer.render_primitives_overlay(target, primitives);
        }
    }

    /// Draw the text decorations belonging to one paint layer.
    pub(super) fn render_text_decorations_for_layer(
        &mut self,
        target: &wgpu::TextureView,
        decorations_by_layer: &std::collections::HashMap<u32, Vec<GpuPrimitive>>,
        z_layer: u32,
    ) {
        if let Some(primitives) = decorations_by_layer.get(&z_layer) {
            if !primitives.is_empty() {
                self.renderer.render_primitives_overlay(target, primitives);
            }
        }
    }

    /// Render a tree with the current overlay state.
    pub fn render_tree_with_state(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        // First render the tree as normal
        self.render_tree(tree, width, height, target)?;

        // Then render overlays from RenderState
        self.render_overlays(render_state, width, height, target);

        Ok(())
    }

    /// Render a tree with motion state using the direct rendering path.
    pub fn render_tree_with_motion(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        self.render_tree_with_motion_opt(tree, render_state, width, height, target, None, false)
    }
}

/// Convert layout's GenericFont to GPU's GenericFont
pub(super) fn to_gpu_generic_font(generic: GenericFont) -> GpuGenericFont {
    match generic {
        GenericFont::System => GpuGenericFont::System,
        GenericFont::Monospace => GpuGenericFont::Monospace,
        GenericFont::Serif => GpuGenericFont::Serif,
        GenericFont::SansSerif => GpuGenericFont::SansSerif,
    }
}

/// Generate text decoration primitives (strikethrough and underline) grouped by z-layer
///
/// Creates decoration lines for text elements that have:
/// - strikethrough: horizontal line through the middle of the text
/// - underline: horizontal line below the text baseline
///
/// Returns a HashMap of z_index -> primitives for interleaved rendering with text
pub(super) fn generate_text_decoration_primitives_by_layer(
    texts: &[TextElement],
) -> std::collections::HashMap<u32, Vec<GpuPrimitive>> {
    let mut primitives_by_layer: std::collections::HashMap<u32, Vec<GpuPrimitive>> =
        std::collections::HashMap::new();

    for text in texts {
        if !text.strikethrough && !text.underline {
            continue;
        }

        // Calculate text width for decorations
        let decoration_width = if text.wrap && text.measured_width > text.width {
            text.width
        } else {
            text.measured_width.min(text.width)
        };

        // Skip if there's no meaningful width
        if decoration_width <= 0.0 {
            continue;
        }

        // Line thickness: use CSS text-decoration-thickness if set, else scale with font size
        let line_thickness = text
            .decoration_thickness
            .unwrap_or_else(|| (text.font_size / 14.0).clamp(1.0, 3.0));

        // Decoration color: use CSS text-decoration-color if set, else use text color
        let dec_color = text.decoration_color.unwrap_or(text.color);

        let layer_primitives = primitives_by_layer.entry(text.z_index).or_default();

        // Match text anchoring when deriving decoration baselines; approximate descenders
        // from the ascender.
        let descender_approx = -text.ascender * 0.2;
        let glyph_extent = text.ascender - descender_approx;

        let baseline_y = match text.v_align {
            TextVerticalAlign::Center => {
                // GPU: y_pos = text.y + text.height / 2.0, then y_offset = y_pos - glyph_extent / 2.0
                // Glyph top is at: text.y + text.height/2 - glyph_extent/2
                // Baseline is at: glyph_top + ascender
                let glyph_top = text.y + text.height / 2.0 - glyph_extent / 2.0;
                glyph_top + text.ascender
            }
            TextVerticalAlign::Top => {
                // GPU: y_pos = text.y, y_offset = y + (layout_height - glyph_extent) / 2.0
                // Glyph top is at: text.y + (text.height - glyph_extent) / 2.0
                // Baseline is at: glyph_top + ascender
                let glyph_top = text.y + (text.height - glyph_extent) / 2.0;
                glyph_top + text.ascender
            }
            TextVerticalAlign::Baseline => {
                // GPU: y_pos = text.y + ascender, y_offset = y_pos - ascender = text.y
                // Glyph top is at: text.y
                // Baseline is at: text.y + ascender
                text.y + text.ascender
            }
        };

        // Strikethrough: draw line through the center of lowercase letters (x-height center)
        if text.strikethrough {
            // x-height is typically ~50% of ascender, center of x-height is ~25% above baseline
            let strikethrough_y = baseline_y - text.ascender * 0.35;
            let mut strike_rect = GpuPrimitive::rect(
                text.x,
                strikethrough_y - line_thickness / 2.0,
                decoration_width,
                line_thickness,
            )
            .with_color(dec_color[0], dec_color[1], dec_color[2], dec_color[3]);

            // Apply clip bounds from text element if present
            if let Some(clip) = text.clip_bounds {
                strike_rect = strike_rect.with_clip_rect(clip[0], clip[1], clip[2], clip[3]);
            }
            layer_primitives.push(strike_rect);
        }

        // Underline: draw line just below the baseline (at text bottom)
        if text.underline {
            // Underline position: just below baseline, snapping to text bottom
            let underline_y = baseline_y + text.ascender * 0.05;
            let mut underline_rect = GpuPrimitive::rect(
                text.x,
                underline_y - line_thickness / 2.0,
                decoration_width,
                line_thickness,
            )
            .with_color(dec_color[0], dec_color[1], dec_color[2], dec_color[3]);

            // Apply clip bounds from text element if present
            if let Some(clip) = text.clip_bounds {
                underline_rect = underline_rect.with_clip_rect(clip[0], clip[1], clip[2], clip[3]);
            }
            layer_primitives.push(underline_rect);
        }
    }

    primitives_by_layer
}
