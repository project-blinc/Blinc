//! Dispatch foreground, dynamic and motion overlay streams.
use super::composite_replay::{ALL_Z, MotionBand};
use super::{
    Brush, CanvasOverlay, Color, CornerRadius, DrawCommand, GpuPaintContext, Overlay,
    PrimitiveBatch, Rect, RenderContext, Stroke,
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
            if batch.has_grouped_glass() {
                if let Some(backdrop) = &self.backdrop_texture {
                    self.renderer
                        .render_overlay_with_glass(target_view, &backdrop.view, &batch);
                }
            } else {
                self.renderer.render_overlay_msaa(target_view, &batch, 1);
            }
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
        self.dispatch_dynamic_batch_around(target_view, |_| {});
    }

    /// Draw the dynamic owner with the motion content that is not in the foreground layer in
    /// paint order: each run of the batch's primitives on one paint layer, then the motion
    /// images, text and SVGs of that layer, then the batch's foreground primitives. A toast or a
    /// menu is on a higher layer or in the foreground, so text below it stays below it.
    pub(super) fn dispatch_dynamic_batch_and_motion(
        &mut self,
        tree: &blinc_layout::RenderTree,
        target_view: &wgpu::TextureView,
    ) {
        let layers = self
            .cached_dynamic_batch
            .as_ref()
            .and_then(|batch| self.dynamic_batch_layers(batch));
        let Some(layers) = layers else {
            self.dispatch_dynamic_batch_around(target_view, |this| {
                this.dispatch_motion_subtree_overlay_pools(
                    tree,
                    target_view,
                    MotionBand::Below,
                    ALL_Z,
                );
            });
            return;
        };
        self.rebind_glyph_atlas_for_overlay();
        let moved = self.resolve_dynamic_batch();
        for (i, (range, _)) in layers.iter().enumerate() {
            self.rebind_glyph_atlas_for_overlay();
            if let Some(batch) = self.cached_dynamic_batch.as_ref() {
                self.renderer
                    .render_overlay_part(target_view, batch, range.clone());
            }
            // The first layer's motion content also takes what is below it, the last's what is
            // above it.
            let low = if i == 0 { 0 } else { layers[i].1 };
            let high = layers.get(i + 1).map_or(u32::MAX, |next| next.1 - 1);
            self.dispatch_motion_subtree_overlay_pools(
                tree,
                target_view,
                MotionBand::Below,
                low..=high,
            );
        }
        self.dispatch_dynamic_foreground(target_view);
        self.restore_dynamic_batch(moved);
    }

    /// The runs of the dynamic batch's primitives on one paint layer, with that layer, when
    /// there is more than one and the batch can be drawn in those parts.
    fn dynamic_batch_layers(
        &self,
        batch: &PrimitiveBatch,
    ) -> Option<Vec<(std::ops::Range<usize>, u32)>> {
        let mut layers: Vec<(std::ops::Range<usize>, u32)> = Vec::new();
        for (i, p) in batch.primitives.iter().enumerate() {
            let z = p.z_layer();
            match layers.last_mut() {
                Some((range, layer)) if *layer == z => range.end = i + 1,
                // Layers that come back down are not drawn in parts.
                Some((_, layer)) if *layer > z => return None,
                _ => layers.push((i..i + 1, z)),
            }
        }
        if layers.len() < 2 {
            return None;
        }
        layers.first_mut()?.0.start = 0;
        let cuts: Vec<usize> = layers[1..].iter().map(|(range, _)| range.start).collect();
        self.renderer
            .overlay_divides_at(batch, &cuts)
            .then_some(layers)
    }

    fn dispatch_dynamic_batch_around(
        &mut self,
        target_view: &wgpu::TextureView,
        between: impl FnOnce(&mut Self),
    ) {
        self.rebind_glyph_atlas_for_overlay();
        let moved = self.resolve_dynamic_batch();
        if let Some(batch) = self.cached_dynamic_batch.as_ref()
            && (!batch.primitives.is_empty() || batch.has_paths() || batch.has_ordered_draws())
        {
            if batch.requires_ordered_rendering() {
                // Keep paths and group commands together. One sample retains
                // SDF coverage across opacity/effect boundaries.
                self.renderer.render_overlay_msaa(target_view, batch, 1);
            } else {
                self.renderer.render_overlay(target_view, batch);
            }
        }
        between(self);
        self.dispatch_dynamic_foreground(target_view);
        self.restore_dynamic_batch(moved);
    }

    /// The dynamic batch's foreground primitives.
    fn dispatch_dynamic_foreground(&mut self, target_view: &wgpu::TextureView) {
        if let Some(batch) = self.cached_dynamic_batch.as_ref()
            && batch.has_foreground_content()
        {
            // What was drawn before bound its own glyph atlas and auxiliary rows.
            if let (Some(atlas), Some(color_atlas)) =
                (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
            {
                self.renderer.set_glyph_atlas(atlas, color_atlas);
            }
            self.renderer.update_aux_data_for_batch(batch);
            self.renderer.render_foreground(target_view, batch);
        }
    }

    /// Draw the scrollbars of the scroll containers on rows, which are not in the batch: their
    /// place and fade come from the physics, and change with every scroll.
    pub(super) fn draw_scroll_row_scrollbars(
        &mut self,
        tree: &blinc_layout::RenderTree,
        target_view: &wgpu::TextureView,
    ) {
        if !tree.scroll_rows_enabled() {
            return;
        }
        let (width, height) = self.renderer.viewport_size();
        let mut ctx = GpuPaintContext::new(width as f32, height as f32);
        let scale = tree.scale_factor();
        {
            use blinc_core::DrawContext;
            if scale != 1.0 {
                ctx.push_transform(blinc_core::Transform::scale(scale, scale));
            }
        }
        tree.draw_scrollbars(&mut ctx);
        let batch = ctx.take_batch();
        if !batch.primitives.is_empty() {
            self.renderer.render_overlay(target_view, &batch);
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
