//! Keep canvas groups, paths and RGBA draws in their original paint order.
use super::{CanvasOverlay, PrimitiveBatch, RenderContext};

impl CanvasOverlay {
    pub(super) fn finish_batches(&mut self, batches: Vec<PrimitiveBatch>) {
        let ordered = batches
            .iter()
            .any(|batch| batch.requires_ordered_rendering() || batch.has_paths());
        for mut batch in batches {
            let shift = self.aux_data.len() as f32;
            RenderContext::shift_aux_offsets(&mut batch.foreground_primitives, shift);
            self.foreground_primitives
                .append(&mut batch.foreground_primitives);
            if ordered {
                self.aux_data.extend_from_slice(&batch.aux_data);
                let mut images = Vec::new();
                for image in batch.dynamic_images.drain(..) {
                    if image.paint_order.is_some() {
                        images.push(image);
                    } else {
                        self.dynamic_images.push(image);
                    }
                }
                batch.dynamic_images = images;
                self.ordered_batches.push(batch);
            } else {
                RenderContext::shift_aux_offsets(&mut batch.primitives, shift);
                self.aux_data.extend(batch.aux_data);
                self.primitives.extend(batch.primitives);
                self.dynamic_images.extend(batch.dynamic_images);
            }
        }
    }
}

impl RenderContext {
    pub(super) fn dispatch_ordered_canvas_batches(
        &mut self,
        target: &wgpu::TextureView,
        overlay: &CanvasOverlay,
    ) {
        if overlay.ordered_batches.is_empty() {
            return;
        }
        self.rebind_glyph_atlas_for_overlay();
        for batch in &overlay.ordered_batches {
            self.renderer.render_overlay_msaa(target, batch, 1);
        }
    }
}
