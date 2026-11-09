//! What the renderer frees once frames stop needing it.

use super::GpuRenderer;

impl GpuRenderer {
    /// Starts a frame: frees the overlay targets no frame has drawn for a while, and moves the
    /// layer texture pool's idle clock, which otherwise moves only while layered passes run. Call
    /// it once per frame; a renderer that is never told keeps what it holds.
    pub fn next_frame(&mut self) {
        self.overlay_targets.next_frame();
        self.layer_texture_cache.evict_oversized();
    }

    /// Frees what the renderer keeps between frames to draw the next one faster: the static
    /// layer, the overlay targets and the pooled layer textures. Call it when the app has gone
    /// idle; the next frame allocates what it needs again and paints in full.
    pub fn release_idle_memory(&mut self) {
        self.static_layer = None;
        self.overlay_targets.clear();
        self.layer_texture_cache.clear_pools();
        // wgpu destroys dropped resources when the device is next maintained, which an idle
        // app does not otherwise do.
        let _ = self.device.poll(wgpu::PollType::Wait);
    }
}
