//! Foreground composition preserves shape groups and their auxiliary data.
use super::{GpuRenderer, PrimitiveBatch};

impl GpuRenderer {
    /// Render foreground shapes and paths once, after background/glass dispatch.
    /// Separate text and raw RGBA images retain their own dispatch contracts.
    pub fn render_foreground(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        if !batch.has_foreground_content() {
            return;
        }
        if !batch.has_foreground_layers() && batch.foreground_paths.indices.is_empty() {
            self.update_aux_data_for_batch(batch);
            self.render_primitives_overlay(target, &batch.foreground_primitives);
            return;
        }
        let foreground = batch.foreground_batch();
        self.render_overlay_msaa(target, &foreground, 1);
    }
}
