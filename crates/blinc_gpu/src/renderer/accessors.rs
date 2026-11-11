//! Device handles, capabilities, viewport, scissor and blend state, the glyph atlas, and the memory budget.

use super::{ActiveGlyphAtlas, GpuMemoryBudget, GpuRenderer};
use std::sync::Arc;

impl GpuRenderer {
    /// Whether the GPU adapter supports storage buffers.
    pub fn has_storage_buffers(&self) -> bool {
        self.has_storage_buffers
    }

    /// Whether the GPU device has `TEXTURE_COMPRESSION_BC` enabled.
    /// Callers that upload material textures should query this to
    /// decide between BC (when `true`) and Rgba8 fallback paths.
    pub fn has_texture_compression_bc(&self) -> bool {
        self.has_texture_compression_bc
    }

    /// Resize the viewport
    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport_size = (width, height);
    }

    /// Current viewport dimensions in physical pixels — width, height.
    /// Used by the compositor v2 damage-rect path on the app side to
    /// clamp scissor rects to the static-layer extent before staging.
    pub fn viewport_size(&self) -> (u32, u32) {
        self.viewport_size
    }

    /// Stage a scissor rect that subsequent text / image dispatches
    /// will apply via `set_scissor_rect`. Used by the compositor v2
    /// damage-rect path to confine text/SVG/image re-dispatches to
    /// the same region the SDF clear + redraw covered. Stays in
    /// effect until [`Self::clear_pending_scissor`] is called.
    pub fn set_pending_scissor(&mut self, rect: (u32, u32, u32, u32)) {
        self.pending_scissor = Some(rect);
    }

    /// Damage-scissor variant of [`Self::set_pending_scissor`]: the
    /// rect is *intersected* with `blit_tight_texture_to_target`'s
    /// own visible-bounds scissor instead of replacing it. Phase 4d
    /// Opt 2 sets this before re-rendering a CSS-animated cache
    /// region so layer-effect composites paint only inside the
    /// damage rect, leaving the rest of the cache from last paint
    /// untouched. Call [`Self::clear_pending_damage_scissor`] when
    /// done.
    pub fn set_pending_damage_scissor(&mut self, rect: (u32, u32, u32, u32)) {
        self.pending_damage_scissor = Some(rect);
    }

    /// Clear the damage scissor — subsequent layer composites use
    /// their own visible-bounds scissor only.
    pub fn clear_pending_damage_scissor(&mut self) {
        self.pending_damage_scissor = None;
    }

    /// Drop the staged scissor so subsequent dispatches paint to the
    /// full attachment again. Paired with `set_pending_scissor`.
    pub fn clear_pending_scissor(&mut self) {
        self.pending_scissor = None;
    }

    /// Set the current render target texture for blend mode two-pass compositing.
    ///
    /// Must be called before `render_overlay()` when the batch may contain
    /// non-Normal blend modes. The texture must remain valid until
    /// `clear_blend_target()` is called.
    ///
    /// # Safety contract
    /// The caller guarantees the texture reference outlives the render frame.
    /// The pointer is only dereferenced within `blit_texture_to_target`.
    pub fn set_blend_target(&mut self, texture: &wgpu::Texture) {
        self.blend_target_ptr = Some(texture as *const wgpu::Texture);
    }

    /// Clear the blend target texture reference after rendering.
    pub fn clear_blend_target(&mut self) {
        self.blend_target_ptr = None;
    }

    /// Update the frame time (for animations)
    pub fn update_time(&mut self, time: f32) {
        self.time = time;
    }

    /// Get the wgpu device
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Get the wgpu device as Arc
    pub fn device_arc(&self) -> Arc<wgpu::Device> {
        self.device.clone()
    }

    /// Get the wgpu queue
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Get the wgpu queue as Arc
    pub fn queue_arc(&self) -> Arc<wgpu::Queue> {
        self.queue.clone()
    }

    /// Surface texture format the renderer was configured with. Used by
    /// the custom-pass dispatch site so user passes know what format
    /// their `initialize` should target.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.texture_format
    }

    /// The adapter the device/queue were created against. Needed for
    /// `Surface::get_capabilities` so callers can negotiate format /
    /// alpha mode / present mode against what the OS compositor
    /// actually exposes.
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    /// Get the texture format used by this renderer's pipelines
    pub fn texture_format(&self) -> wgpu::TextureFormat {
        self.texture_format
    }

    /// Returns true if unified text/SDF rendering is enabled
    ///
    /// When enabled, text glyphs are converted to SDF primitives and rendered
    /// in the same GPU pass as other shapes, ensuring consistent transforms
    /// during animations.
    pub fn unified_text_rendering(&self) -> bool {
        self.config.unified_text_rendering
    }

    /// Poll the device to process completed GPU operations and free resources.
    /// Call this after frame rendering to prevent memory accumulation.
    pub fn poll(&self) {
        // wgpu 26: `Maintain::Wait` was renamed to `PollType::Wait`. Result
        // is a `Result<PollStatus, _>` rather than the old `MaintainResult`,
        // and we don't care about the precise status here — we just want
        // to block until the GPU is idle.
        //
        // Mobile skips the wait: the swapchain paces frames, and idling the CPU until the
        // GPU finishes serialises encode with execution.
        let poll = if cfg!(any(target_os = "ios", target_os = "android")) {
            wgpu::PollType::Poll
        } else {
            wgpu::PollType::Wait
        };
        let _ = self.device.poll(poll);
    }

    /// Bind real glyph atlas textures into the default SDF bind group.
    ///
    /// Call once per frame before any rendering when CSS-transformed text is present.
    /// This replaces the placeholder atlas with the real glyph atlas in
    /// `self.bind_groups.sdf`, so ALL render paths automatically get the atlas
    /// without needing to thread it through every method.
    ///
    /// Always rebuilds the bind group. The previous pointer-equality
    /// optimisation was a false-negative trap: `text_ctx` replaces
    /// the `atlas_view` field *in place* (same `Option<TextureView>`
    /// memory address, different inner `Arc`) when the atlas grows.
    /// The borrowed reference `atlas_view as *const TextureView`
    /// returned the same pointer before and after growth, so the
    /// check thought "no change" while the underlying view was a
    /// different texture — the bind group kept the OLD view's Arc
    /// and subsequent glyph samples landed in the old atlas.
    ///
    /// Symptom: canvas_demo `ctx.draw_text` rendered blank because
    /// `collect_canvas_overlay` grew the atlas after the slow
    /// path's `set_glyph_atlas` call, the bind group stayed on the
    /// pre-growth view, and the new glyph UVs (referring to the
    /// post-growth atlas) sampled from the wrong texture.
    ///
    /// Cost of always rebuilding: one `create_bind_group` call
    /// (~50 µs) per `set_glyph_atlas` invocation. Per frame on the
    /// slow path that's negligible; the overlay-rebind helper
    /// `BlincApp::rebind_glyph_atlas_for_overlay` calls this once
    /// per `composite_frame` site, so total cost is one extra
    /// rebuild per frame at most.
    ///
    /// SAFETY: The raw pointers stored in `active_glyph_atlas` must
    /// remain valid for the duration of the frame — guaranteed
    /// because they point to TextureViews owned by the text
    /// context, which outlives all render calls.
    pub fn set_glyph_atlas(
        &mut self,
        atlas_view: &wgpu::TextureView,
        color_atlas_view: &wgpu::TextureView,
    ) {
        let atlas_ptr = atlas_view as *const wgpu::TextureView;
        let color_ptr = color_atlas_view as *const wgpu::TextureView;
        self.active_glyph_atlas = Some(ActiveGlyphAtlas {
            atlas_view_ptr: atlas_ptr,
            color_atlas_view_ptr: color_ptr,
        });
        self.rebind_sdf_bind_group();
    }

    /// Enforce the GPU memory budget by evicting cached textures.
    ///
    /// Call once per frame (e.g., at frame start) to keep memory in check.
    /// Evicts largest pooled textures first, then trims mask image cache
    /// if still over budget.
    pub fn enforce_memory_budget(&mut self) {
        self.memory_budget.reset_transient();

        if self.memory_budget.budget() == 0 {
            return; // unlimited
        }

        let layer_bytes = self.layer_texture_cache.stats().total_memory_bytes();
        if !self.memory_budget.is_over_budget(layer_bytes) {
            return;
        }

        // Phase 1: evict pooled layer textures (largest first)
        let target = self
            .memory_budget
            .budget()
            .saturating_sub(self.memory_budget.mask_image_bytes);
        let freed = self.layer_texture_cache.evict_to_budget(target);
        if freed > 0 {
            self.memory_budget.record_eviction();
        }

        // Phase 2: if still over, trim mask image cache (drop oldest entries)
        let layer_bytes = self.layer_texture_cache.stats().total_memory_bytes();
        if self.memory_budget.is_over_budget(layer_bytes) && !self.mask_image_cache.is_empty() {
            // Remove one entry at a time until under budget
            let keys: Vec<String> = self.mask_image_cache.keys().cloned().collect();
            for key in keys {
                if !self
                    .memory_budget
                    .is_over_budget(self.layer_texture_cache.stats().total_memory_bytes())
                {
                    break;
                }
                if let Some(img) = self.mask_image_cache.remove(&key) {
                    let (w, h) = img.dimensions();
                    self.memory_budget.untrack_mask_image(w, h);
                    self.memory_budget.record_eviction();
                }
            }
        }
    }

    /// Get the current GPU memory budget tracker.
    pub fn memory_budget(&self) -> &GpuMemoryBudget {
        &self.memory_budget
    }

    /// Get estimated total GPU texture memory usage in bytes.
    pub fn estimated_texture_memory(&self) -> u64 {
        let layer_bytes = self.layer_texture_cache.stats().total_memory_bytes();
        self.memory_budget.total_tracked_bytes(layer_bytes)
    }
}
