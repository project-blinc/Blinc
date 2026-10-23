//! Dispatch foreground, dynamic and motion overlay streams.
use super::{
    Brush, CanvasOverlay, Color, CornerRadius, DebugMode, DrawCommand, GpuPaintContext,
    GpuPrimitive, Overlay, PrimitiveBatch, Rect, RenderContext, RenderState, RenderTree, Result,
    Stroke, TextAlign, TextAlignment, TextAnchor, TextVerticalAlign, text_clip,
    to_gpu_generic_font,
};

impl RenderContext {
    /// Bind the latest atlas views before drawing overlay glyphs; canvas callbacks may grow
    /// the atlas.
    pub(super) fn rebind_glyph_atlas_for_overlay(&mut self) {
        if let (Some(atlas), Some(color_atlas)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            self.renderer.set_glyph_atlas(atlas, color_atlas);
        }
    }

    /// Draw cached foreground, foreground text and canvas foreground above ordinary canvas
    /// output. Global overlays follow.
    pub(super) fn render_foreground_overlay(
        &mut self,
        target_view: &wgpu::TextureView,
        canvas_overlays: &[&CanvasOverlay],
    ) {
        let foreground = self
            .cached_bg_batch
            .as_ref()
            .filter(|batch| batch.has_foreground_content())
            .map(PrimitiveBatch::foreground_batch);
        let fg_glyphs = self
            .cached_fg_glyphs
            .as_ref()
            .filter(|v| !v.is_empty())
            .cloned();
        let has_canvas_fg = canvas_overlays.iter().any(|overlay| {
            !overlay.foreground_primitives.is_empty() || !overlay.foreground_batches.is_empty()
        });
        if foreground.is_none() && fg_glyphs.is_none() && !has_canvas_fg {
            return;
        }
        self.rebind_glyph_atlas_for_overlay();
        if let Some(batch) = foreground {
            self.renderer.render_overlay_msaa(target_view, &batch, 1);
        }
        // Replay direct and converted foreground text with its own rows
        // after canvas content, matching the separate foreground stream.
        if let Some(glyphs) = fg_glyphs {
            self.render_text(target_view, &glyphs);
        }
        for canvas_overlay in canvas_overlays {
            self.dispatch_canvas_foreground_batches(target_view, canvas_overlay);
            if canvas_overlay.foreground_primitives.is_empty() {
                continue;
            }
            // Restore the canvas auxiliary buffer after foreground/text dispatch.
            if !canvas_overlay.aux_data.is_empty() {
                self.renderer
                    .update_aux_data_slice(&canvas_overlay.aux_data);
            }
            self.rebind_glyph_atlas_for_overlay();
            self.renderer
                .render_primitives_overlay(target_view, &canvas_overlay.foreground_primitives);
        }
    }

    /// Draw the dynamic owner in a separate pass with its own auxiliary rows.
    pub(super) fn dispatch_dynamic_batch_overlay(&mut self, target_view: &wgpu::TextureView) {
        self.rebind_glyph_atlas_for_overlay();
        if let Some(batch) = self.cached_dynamic_batch.as_ref() {
            if !batch.primitives.is_empty() || batch.has_paths() || batch.has_ordered_draws() {
                if batch.requires_ordered_rendering() {
                    // Keep paths and group commands together. One sample retains
                    // SDF coverage across opacity/effect boundaries.
                    self.renderer.render_overlay_msaa(target_view, batch, 1);
                } else {
                    self.renderer.render_overlay(target_view, batch);
                }
            }
            self.renderer.render_foreground(target_view, batch);
        }
    }

    /// Draw overlay-subtree canvases above their dynamic host geometry.
    pub(super) fn dispatch_overlay_canvas_overlay(
        &mut self,
        target_view: &wgpu::TextureView,
        overlay: &CanvasOverlay,
    ) {
        if overlay.primitives.is_empty()
            && overlay.foreground_primitives.is_empty()
            && overlay.ordered_batches.is_empty()
            && overlay.foreground_batches.is_empty()
            && overlay.dynamic_images.is_empty()
        {
            return;
        }
        if !overlay.aux_data.is_empty() {
            self.renderer.update_aux_data_slice(&overlay.aux_data);
        }
        self.rebind_glyph_atlas_for_overlay();
        if !overlay.primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target_view, &overlay.primitives);
        }
        self.dispatch_ordered_canvas_batches(target_view, overlay);
        self.renderer
            .render_unordered_dynamic_images(target_view, &overlay.dynamic_images);
        self.dispatch_canvas_foreground_batches(target_view, overlay);
        if !overlay.foreground_primitives.is_empty() {
            self.renderer.update_aux_data_slice(&overlay.aux_data);
            self.renderer
                .render_primitives_overlay(target_view, &overlay.foreground_primitives);
        }
    }

    /// Render an overlay tree with its live motion and collected content.
    pub fn render_overlay_tree_with_motion(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        // Get scale factor for HiDPI rendering
        let scale_factor = tree.scale_factor();

        // Create a single paint context for all layers with text rendering support
        let mut ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);

        // Render with motion animations applied (all layers to same context)
        tree.render_with_motion(&mut ctx, render_state);

        // Take the batch (mutable so CSS-transformed text primitives can be added)
        let mut batch = ctx.take_batch();

        // Collect text, SVG, image, and flow elements WITH motion state
        let (texts, svgs, images, _flows) =
            self.collect_render_elements_with_state(tree, Some(render_state));

        // Pre-load all images into cache before rendering
        self.preload_images(&images, width as f32, height as f32);

        // Prepare text glyphs with z_layer information
        let mut glyphs_by_layer: std::collections::BTreeMap<u32, PrimitiveBatch> =
            std::collections::BTreeMap::new();
        let mut css_text = PrimitiveBatch::new();
        for text in &texts {
            let alignment = match text.align {
                TextAlign::Left => TextAlignment::Left,
                TextAlign::Center => TextAlignment::Center,
                TextAlign::Right => TextAlignment::Right,
            };

            // Apply motion opacity to text color
            let color = if text.motion_opacity < 1.0 {
                [
                    text.color[0],
                    text.color[1],
                    text.color[2],
                    text.color[3] * text.motion_opacity,
                ]
            } else {
                text.color
            };

            // Determine wrap width
            let effective_width = if let Some(clip) = text.clip_bounds {
                clip[2].min(text.width)
            } else {
                text.width
            };

            let needs_wrap = text.wrap && effective_width < text.measured_width - 2.0;
            let wrap_width = Some(text.width);
            let font_name = text.font_family.name.as_deref();
            let generic = to_gpu_generic_font(text.font_family.generic);
            let font_weight = text.weight.weight();

            let (anchor, y_pos, use_layout_height) = match text.v_align {
                TextVerticalAlign::Center => {
                    (TextAnchor::Center, text.y + text.height / 2.0, false)
                }
                TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                TextVerticalAlign::Baseline => {
                    let baseline_y = text.y + text.half_leading + text.ascender;
                    (TextAnchor::Baseline, baseline_y, false)
                }
            };
            let layout_height = if use_layout_height {
                Some(text.height)
            } else {
                None
            };

            if let Ok(glyphs) = self.text_ctx.prepare_text_with_style(
                &text.content,
                text.x,
                y_pos,
                text.font_size,
                color,
                anchor,
                alignment,
                wrap_width,
                needs_wrap,
                font_name,
                generic,
                font_weight,
                text.italic,
                layout_height,
                text.letter_spacing,
            ) {
                let mut owned = text_clip::prepare_glyphs(text, glyphs, (width, height));
                if let Some(affine) = text.css_affine {
                    // CSS-transformed text: convert to SDF primitives with local_affine
                    // Join the walker batch with the glyphs' own auxiliary rows.
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
                        prim.set_z_layer(text.z_index);
                        owned.primitives.push(prim);
                    }
                    owned.glyphs.clear();
                    css_text.merge(owned);
                } else {
                    glyphs_by_layer
                        .entry(text.z_index)
                        .or_default()
                        .merge(owned);
                }
            }
        }

        // SVGs are rendered as rasterized images (not tessellated paths) for better anti-aliasing
        // They will be rendered later via render_rasterized_svgs

        self.renderer.resize(width, height);

        // Bind the current atlas for canvas glyph primitives.
        if let (Some(atlas), Some(color_atlas)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            batch.merge(css_text);
            self.renderer.set_glyph_atlas(atlas, color_atlas);
        }

        // For overlay rendering, we DON'T have glass effects (overlays are simple)
        // Render primitives without clearing (LoadOp::Load)
        let max_z = batch.max_z_layer();
        let max_text_z = glyphs_by_layer.keys().cloned().max().unwrap_or(0);
        let max_layer = max_z.max(max_text_z);

        tracing::trace!(
            "render_overlay_tree: {} primitives, {} text layers, max_layer={}",
            batch.primitives.len(),
            glyphs_by_layer.len(),
            max_layer
        );

        // Render all layers using overlay mode (no clear)
        for z in 0..=max_layer {
            let layer_primitives = batch.primitives_for_layer(z);
            if !layer_primitives.is_empty() {
                tracing::trace!(
                    "render_overlay_tree: rendering {} primitives at z={}",
                    layer_primitives.len(),
                    z
                );
                self.renderer.update_aux_data_for_batch(&batch);
                self.renderer
                    .render_primitives_overlay(target, &layer_primitives);
            }

            if let Some(glyphs) = glyphs_by_layer.get(&z) {
                if !glyphs.glyphs.is_empty() {
                    tracing::trace!(
                        "render_overlay_tree: rendering {} glyphs at z={}",
                        glyphs.glyphs.len(),
                        z
                    );
                    self.render_owned_text(target, glyphs);
                }
            }
        }

        // Images render on top
        self.render_images(target, &images, width as f32, height as f32, scale_factor);

        // Render foreground primitives (e.g. borders on top)
        if batch.has_foreground_content() {
            self.renderer.render_foreground(target, &batch);
        }

        // Poll the device to free completed command buffers
        self.renderer.poll();

        // Render layout debug for overlay tree if enabled
        let debug = DebugMode::from_env();
        if debug.layout {
            let scale = tree.scale_factor();
            self.render_layout_debug(target, tree, scale);
        }
        if debug.motion {
            self.render_motion_debug(target, tree, width, height);
        }

        // Return scratch buffers for reuse on next frame
        self.return_scratch_elements(texts, svgs, images);

        Ok(())
    }

    /// Draw state-managed overlay content above the current frame.
    pub(super) fn render_overlays(
        &mut self,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) {
        let overlays = render_state.overlays();
        if overlays.is_empty() {
            return;
        }

        // Create a paint context for overlays
        let mut overlay_ctx = GpuPaintContext::new(width as f32, height as f32);

        for overlay in overlays {
            match overlay {
                Overlay::Cursor {
                    position,
                    size,
                    color,
                    opacity,
                } => {
                    if *opacity > 0.0 {
                        // Apply opacity to cursor color
                        let cursor_color =
                            Color::rgba(color.r, color.g, color.b, color.a * opacity);
                        overlay_ctx.execute_command(&DrawCommand::FillRect {
                            rect: Rect::new(position.0, position.1, size.0, size.1),
                            corner_radius: CornerRadius::default(),
                            brush: Brush::Solid(cursor_color),
                        });
                    }
                }
                Overlay::Selection { rects: _, color: _ } => {
                    // TODO: Re-enable for real-time text selection
                    // Disabled for now to avoid blue mask issue after modal close
                }
                Overlay::FocusRing {
                    position,
                    size,
                    radius,
                    color,
                    thickness,
                } => {
                    overlay_ctx.execute_command(&DrawCommand::StrokeRect {
                        rect: Rect::new(position.0, position.1, size.0, size.1),
                        corner_radius: CornerRadius::uniform(*radius),
                        stroke: Stroke::new(*thickness),
                        brush: Brush::Solid(*color),
                    });
                }
            }
        }

        // Render overlays as an overlay pass (on top of existing content)
        let overlay_batch = overlay_ctx.take_batch();
        if !overlay_batch.is_empty() {
            self.renderer.render_overlay(target, &overlay_batch);
        }
    }
}
