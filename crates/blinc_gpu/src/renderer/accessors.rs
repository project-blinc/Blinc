//! Device handles, capabilities, viewport, scissor and blend state, the glyph atlas, and the memory budget.

use super::{ActiveGlyphAtlas, GpuMemoryBudget, GpuRenderer};
use crate::node_table::{NODE_TABLE_CAPACITY, NodeRow};
use std::sync::Arc;

impl GpuRenderer {
    /// Whether the GPU adapter supports storage buffers.
    pub fn has_storage_buffers(&self) -> bool {
        self.has_storage_buffers
    }

    /// Whether the animated node table is bound. Primitives name a row through `node_slot`.
    pub fn node_table_enabled(&self) -> bool {
        self.node_table
    }

    /// Replace the node table's rows. Returns `false` and leaves the table alone when it is
    /// off, or `rows` is longer than [`NODE_TABLE_CAPACITY`]. Rows past the end of `rows`
    /// keep what they held, so a primitive must not name one.
    pub fn set_node_rows(&self, rows: &[NodeRow]) -> bool {
        let Some(buffer) = self.buffers.node_rows.as_ref() else {
            return false;
        };
        if rows.len() > NODE_TABLE_CAPACITY {
            return false;
        }
        if !rows.is_empty() {
            self.uploads.write(
                &self.device,
                &self.queue,
                buffer,
                0,
                bytemuck::cast_slice(rows),
            );
        }
        let mut mirror = self.node_rows_cpu.lock().unwrap();
        mirror.clear();
        mirror.extend_from_slice(rows);
        true
    }

    /// `prim` as it is drawn with its node's rows applied on the CPU and no slot left, for the
    /// passes that shift primitives into another frame before drawing them.
    pub(crate) fn resolve_node_slot(
        &self,
        prim: &crate::primitives::GpuPrimitive,
    ) -> crate::primitives::GpuPrimitive {
        match prim.node_slot() {
            Some(_) => {
                crate::node_table::resolve_primitive(&self.node_rows_cpu.lock().unwrap(), prim)
            }
            None => *prim,
        }
    }

    /// `batch` with the rows applied on the CPU to every primitive that names one, or `batch`
    /// itself when none does.
    pub(crate) fn with_node_rows_applied<'a>(
        &self,
        batch: &'a crate::primitives::PrimitiveBatch,
    ) -> std::borrow::Cow<'a, crate::primitives::PrimitiveBatch> {
        if !batch.primitives.iter().any(|p| p.node_slot().is_some())
            && !batch.dynamic_images.iter().any(|i| i.node_slot.is_some())
        {
            return std::borrow::Cow::Borrowed(batch);
        }
        let mut resolved = batch.clone();
        let rows = self.node_rows_cpu.lock().unwrap();
        let mut aux = std::mem::take(&mut resolved.aux_data);
        for p in &mut resolved.primitives {
            crate::node_table::follow_path_clip(&rows, p, &mut aux);
            if p.node_slot().is_some() {
                *p = crate::node_table::resolve_primitive(&rows, p);
            }
        }
        resolved.aux_data = aux;
        for image in &mut resolved.dynamic_images {
            *image = crate::node_table::place_dynamic_image(&rows, image);
        }
        resolved.node_slots_epoch = 0;
        std::borrow::Cow::Owned(resolved)
    }

    /// Where `prim` is drawn: its bounds, or those bounds moved by its node's rows.
    pub(crate) fn drawn_bounds(&self, prim: &crate::primitives::GpuPrimitive) -> [f32; 4] {
        match prim.node_slot() {
            Some(slot) => crate::node_table::drawn_bounds(
                &self.node_rows_cpu.lock().unwrap(),
                slot,
                prim.bounds,
            ),
            None => prim.bounds,
        }
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

    /// Draw consecutive plain opacity groups one at a time (`false`) or through a shared atlas
    /// (`true`, the default). The two paint the same pixels; the switch is for comparing them.
    pub fn set_batch_opacity_layers(&mut self, on: bool) {
        self.batch_opacity_layers = on;
    }

    /// How many atlases opacity groups have been drawn through since the renderer was made.
    pub fn opacity_atlas_count(&self) -> u64 {
        self.opacity_atlases
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
    #[deprecated(note = "the renderer reads `blinc_core::FrameClock`; runners set it each frame")]
    pub fn update_time(&mut self, _time: f32) {}

    /// Get the wgpu device
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Get the wgpu device as Arc
    pub fn device_arc(&self) -> Arc<wgpu::Device> {
        self.device.clone()
    }

    /// Queue the renderer's submits until [`Self::end_frame_submission`], which submits the
    /// frame at once. Data-texture mode rewrites one texture per pass, so it keeps
    /// submitting each pass.
    pub fn begin_frame_submission(&self) {
        if self.has_storage_buffers {
            self.uploads.begin_frame();
        }
    }

    /// Submit what the frame queued and stop queueing.
    pub fn end_frame_submission(&self) {
        self.uploads.end_frame(&self.queue);
    }

    /// Get the wgpu queue. Submits the renderer's pending buffer writes first, so writes
    /// made through the queue land after them.
    pub fn queue(&self) -> &wgpu::Queue {
        self.uploads.flush(&self.queue);
        &self.queue
    }

    /// Get the wgpu queue as Arc. Submits pending buffer writes first, as [`Self::queue`].
    pub fn queue_arc(&self) -> Arc<wgpu::Queue> {
        self.uploads.flush(&self.queue);
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
    /// Rebuilds the bind group only when the views differ from the bound ones. Views
    /// compare by the resource they name, not by address: the text context replaces its
    /// atlas view in place when the atlas grows, so the address alone stays the same.
    pub fn set_glyph_atlas(
        &mut self,
        atlas_view: &wgpu::TextureView,
        color_atlas_view: &wgpu::TextureView,
    ) {
        let active = ActiveGlyphAtlas {
            atlas_view: atlas_view.clone(),
            color_atlas_view: color_atlas_view.clone(),
        };
        if self.active_glyph_atlas.as_ref() == Some(&active) {
            return;
        }
        self.active_glyph_atlas = Some(active);
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
