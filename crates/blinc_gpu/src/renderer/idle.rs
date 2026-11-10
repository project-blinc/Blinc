//! What the renderer frees once frames stop needing it.

use super::GpuRenderer;

/// How many frames a window-sized intermediate may go unused before it is freed. One used every
/// few frames keeps it.
pub(super) const IDLE_FRAMES: u64 = 60;

impl GpuRenderer {
    /// Starts a frame: frees the overlay targets, the flow scene copy and the mesh pass's
    /// window-sized targets once no frame has used them for a while, and moves the layer texture
    /// pool's idle clock, which otherwise moves only while layered passes run. Call
    /// it once per frame; a renderer that is never told keeps what it holds.
    pub fn next_frame(&mut self) {
        self.overlay_targets.next_frame();
        self.layer_texture_cache.evict_oversized();
        let now = self.overlay_targets.frame();
        if now.saturating_sub(self.scene_copy_last_frame) > IDLE_FRAMES {
            self.scene_copy_texture = None;
        }
        if now.saturating_sub(self.mesh_targets_last_frame) > IDLE_FRAMES
            && let Some(mesh) = self.mesh_pipeline.as_mut()
        {
            mesh.release_targets();
        }
    }

    /// The frame being drawn, for stamping what it uses.
    pub(crate) fn current_frame(&self) -> u64 {
        self.overlay_targets.frame()
    }

    /// Frees what the renderer keeps between frames to draw the next one faster: the static
    /// layer, the overlay targets, the pooled layer textures, the flow scene copy and the mesh
    /// pass's window-sized targets. Call it when the app has gone
    /// idle; the next frame allocates what it needs again and paints in full.
    pub fn release_idle_memory(&mut self) {
        self.static_layer = None;
        self.overlay_targets.clear();
        self.layer_texture_cache.clear_pools();
        self.scene_copy_texture = None;
        if let Some(mesh) = self.mesh_pipeline.as_mut() {
            mesh.release_targets();
        }
        // wgpu destroys dropped resources when the device is next maintained, which an idle
        // app does not otherwise do.
        let _ = self.device.poll(wgpu::PollType::Wait);
    }
}
