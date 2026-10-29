//! Foreground composition preserves image/shape order, groups and auxiliary data.
use super::{GpuRenderer, PrimitiveBatch};

impl GpuRenderer {
    /// Compose ordered glass commands against the supplied frame backdrop.
    pub fn render_overlay_with_glass(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        batch: &PrimitiveBatch,
    ) {
        self.render_layered_overlay_with_backdrop(target, batch, 1, Some(backdrop));
    }

    /// Compose foreground glass groups against this frame's prepared backdrop.
    pub fn render_foreground_with_glass(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        batch: &PrimitiveBatch,
    ) {
        if !batch.has_grouped_glass() {
            self.render_foreground(target, batch);
            return;
        }
        self.render_overlay_with_glass(target, backdrop, &batch.foreground_batch());
    }

    /// Render foreground images, shapes and paths once after background/glass.
    /// Separate prepared text retains its own dispatch contract.
    /// Grouped glass uses `render_foreground_with_glass`.
    pub fn render_foreground(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        if !batch.has_foreground_content() {
            return;
        }
        if !batch.has_foreground_layers()
            && batch.foreground_paths.indices.is_empty()
            && batch.foreground_dynamic_images.is_empty()
        {
            self.update_aux_data_for_batch(batch);
            self.render_primitives_overlay(target, &batch.foreground_primitives);
            return;
        }
        let foreground = batch.foreground_batch();
        self.render_overlay_msaa(target, &foreground, 1);
    }
}
