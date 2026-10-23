//! Cache invalidation, scratch reuse and resource lifecycle.
use super::{
    IMAGE_CACHE_CAPACITY, ImageElement, RenderContext, SVG_CACHE_CAPACITY, SvgElement, TextElement,
};

impl RenderContext {
    /// Invalidate cached frame content while retaining reusable promoted textures.
    pub fn invalidate_render_cache(&mut self) {
        self.invalidate_render_cache_tagged("unspecified");
    }

    pub fn invalidate_render_cache_tagged(&mut self, source: &'static str) {
        if self.cached_bg_batch.is_some() {
            tracing::trace!(
                target: "blinc_app::frame_timing",
                source,
                "invalidate_render_cache",
            );
        }
        self.cached_bg_batch = None;
        self.cached_dynamic_batch = None;
        self.cached_texts = None;
        self.cached_svgs = None;
        self.cached_images = None;
        self.cached_flows = None;
        self.cached_glyphs_by_layer = None;
        self.cached_fg_glyphs = None;
        self.cached_motion_subtree_text_prims = None;
        self.motion_subtree_text_patch.clear();
        self.cached_motion_subtree_svgs = None;
        self.cached_css_transformed_text_prims = None;
        // Compositor static cache rides on the same lifecycle as the
        // primitive-batch cache — anything that would invalidate the
        // batch (rebuild, layout change, scroll, hover state change,
        // CSS state change) also makes the cached texture stale.
        self.renderer.invalidate_static_layer();
        // Keep promoted textures; their scratch hashes determine which bakes need
        // refreshing.
    }

    /// Release promoted textures for the supplied mounted overlay keys.
    pub fn invalidate_overlay_mount_textures(&mut self, keys: &[u64]) {
        if keys.is_empty() {
            return;
        }
        for key in keys {
            if let Some((old, _)) = self.css_composited_textures.remove(key) {
                self.renderer.layer_texture_cache_mut().release(old);
            }
            self.css_composited_scratch_hash.remove(key);
            if let Some((old, _)) = self.motion_subtree_textures.remove(key) {
                self.renderer.layer_texture_cache_mut().release(old);
            }
            self.motion_subtree_scratch_hash.remove(key);
        }
        tracing::trace!(
            target: "blinc_app::frame_timing",
            count = keys.len(),
            "invalidate_overlay_mount_textures",
        );
    }

    /// Release all promoted textures when their shared resources become invalid.
    pub fn drain_all_layer_textures(&mut self, source: &'static str) {
        if self.css_composited_textures.is_empty() && self.motion_subtree_textures.is_empty() {
            return;
        }
        let css_count = self.css_composited_textures.len();
        let motion_count = self.motion_subtree_textures.len();
        for (_, (tex, _)) in self.css_composited_textures.drain() {
            self.renderer.layer_texture_cache_mut().release(tex);
        }
        self.css_composited_scratch_hash.clear();
        for (_, (tex, _)) in self.motion_subtree_textures.drain() {
            self.renderer.layer_texture_cache_mut().release(tex);
        }
        self.motion_subtree_scratch_hash.clear();
        tracing::trace!(
            target: "blinc_app::frame_timing",
            source,
            css_count,
            motion_count,
            "drain_all_layer_textures",
        );
    }

    /// Whether a full paint has populated the primitive cache.
    pub fn has_render_cache(&self) -> bool {
        self.cached_bg_batch.is_some()
    }

    /// Return element vectors to scratch pool for reuse
    #[inline]
    pub(super) fn return_scratch_elements(
        &mut self,
        mut texts: Vec<TextElement>,
        mut svgs: Vec<SvgElement>,
        mut images: Vec<ImageElement>,
    ) {
        // Clear and keep capacity for reuse
        texts.clear();
        svgs.clear();
        images.clear();
        self.scratch_texts = texts;
        self.scratch_svgs = svgs;
        self.scratch_images = images;
    }

    /// Log cache statistics (called every 300 frames, ~5 seconds at 60fps).
    /// Visible at RUST_LOG=blinc_app=debug level.
    pub(super) fn log_cache_stats(&mut self) {
        self.frame_count += 1;
        if self.frame_count % 300 != 1 {
            return;
        }
        let (aw, ah) = self.text_ctx.atlas_dimensions();
        let (caw, cah) = self.text_ctx.color_atlas_dimensions();
        let atlas_glyphs = self.text_ctx.atlas().glyph_count();
        let atlas_util = self.text_ctx.atlas().utilization();
        let color_glyphs = self.text_ctx.color_atlas().glyph_count();
        let color_util = self.text_ctx.color_atlas().utilization();
        let glyph_cache = self.text_ctx.glyph_cache_len();
        let glyph_cap = self.text_ctx.glyph_cache_capacity();
        let color_cache = self.text_ctx.color_glyph_cache_len();
        let color_cap = self.text_ctx.color_glyph_cache_capacity();
        let img_cache = self.image_cache.len();
        let svg_cache = self.svg_cache.len();
        let svg_atlas_entries = self.svg_atlas.entry_count();
        let svg_atlas_util = self.svg_atlas.utilization();
        let (svg_aw, svg_ah) = (self.svg_atlas.width(), self.svg_atlas.height());

        tracing::info!(
            "Cache stats [frame {}]: \
             atlas={}x{} ({} glyphs, {:.1}% used), \
             color_atlas={}x{} ({} glyphs, {:.1}% used), \
             glyph_lru={}/{}, color_glyph_lru={}/{}, \
             image={}/{}, svg_doc={}/{}, svg_atlas={}x{} ({} entries, {:.1}% used)",
            self.frame_count,
            aw,
            ah,
            atlas_glyphs,
            atlas_util * 100.0,
            caw,
            cah,
            color_glyphs,
            color_util * 100.0,
            glyph_cache,
            glyph_cap,
            color_cache,
            color_cap,
            img_cache,
            IMAGE_CACHE_CAPACITY,
            svg_cache,
            SVG_CACHE_CAPACITY,
            svg_aw,
            svg_ah,
            svg_atlas_entries,
            svg_atlas_util * 100.0,
        );
    }
}
