//! Load independent overlay trees through the ordered frame pipeline.
use super::*;

#[derive(Default)]
struct FrameCache {
    cached_glyphs_by_layer: Option<std::collections::BTreeMap<u32, cached_text::TextBatch>>,
    cached_fg_glyphs: Option<cached_text::TextBatch>,
    cached_motion_subtree_text_prims: Option<cached_text::TextBatch>,
    motion_subtree_text_patch: Vec<(std::ops::Range<usize>, blinc_layout::tree::StableNodeId)>,
    composite_patch_baselines: std::collections::HashMap<
        blinc_layout::tree::StableNodeId,
        ([f32; 4], (f32, f32), (f32, f32)),
    >,
    cached_motion_subtree_svgs: Option<Vec<SvgElement>>,
    cached_css_transformed_text_prims: Option<PrimitiveBatch>,
    cached_bg_batch: Option<blinc_gpu::PrimitiveBatch>,
    cached_dynamic_batch: Option<blinc_gpu::PrimitiveBatch>,
    css_composited_textures:
        std::collections::HashMap<u64, (blinc_gpu::renderer::LayerTexture, (u32, u32))>,
    css_composited_scratch_hash: std::collections::HashMap<u64, u64>,
    motion_subtree_textures:
        std::collections::HashMap<u64, (blinc_gpu::renderer::LayerTexture, (u32, u32))>,
    motion_subtree_scratch_hash: std::collections::HashMap<u64, u64>,
    cached_texts: Option<Vec<TextElement>>,
    cached_svgs: Option<Vec<SvgElement>>,
    cached_images: Option<Vec<ImageElement>>,
    captured_images_need_repaint: bool,
    has_active_flows: bool,
}

impl FrameCache {
    fn swap(&mut self, context: &mut RenderContext) {
        std::mem::swap(
            &mut self.cached_glyphs_by_layer,
            &mut context.cached_glyphs_by_layer,
        );
        std::mem::swap(&mut self.cached_fg_glyphs, &mut context.cached_fg_glyphs);
        std::mem::swap(
            &mut self.cached_motion_subtree_text_prims,
            &mut context.cached_motion_subtree_text_prims,
        );
        std::mem::swap(
            &mut self.motion_subtree_text_patch,
            &mut context.motion_subtree_text_patch,
        );
        std::mem::swap(
            &mut self.composite_patch_baselines,
            &mut context.composite_patch_baselines,
        );
        std::mem::swap(
            &mut self.cached_motion_subtree_svgs,
            &mut context.cached_motion_subtree_svgs,
        );
        std::mem::swap(
            &mut self.cached_css_transformed_text_prims,
            &mut context.cached_css_transformed_text_prims,
        );
        std::mem::swap(&mut self.cached_bg_batch, &mut context.cached_bg_batch);
        std::mem::swap(
            &mut self.cached_dynamic_batch,
            &mut context.cached_dynamic_batch,
        );
        std::mem::swap(
            &mut self.css_composited_textures,
            &mut context.css_composited_textures,
        );
        std::mem::swap(
            &mut self.css_composited_scratch_hash,
            &mut context.css_composited_scratch_hash,
        );
        std::mem::swap(
            &mut self.motion_subtree_textures,
            &mut context.motion_subtree_textures,
        );
        std::mem::swap(
            &mut self.motion_subtree_scratch_hash,
            &mut context.motion_subtree_scratch_hash,
        );
        std::mem::swap(&mut self.cached_texts, &mut context.cached_texts);
        std::mem::swap(&mut self.cached_svgs, &mut context.cached_svgs);
        std::mem::swap(&mut self.cached_images, &mut context.cached_images);
        std::mem::swap(
            &mut self.captured_images_need_repaint,
            &mut context.captured_images_need_repaint,
        );
        std::mem::swap(&mut self.has_active_flows, &mut context.has_active_flows);
    }
}

impl RenderContext {
    /// Paint a fresh overlay tree above existing pixels without replacing main-tree replay.
    pub fn render_overlay_tree_with_motion(
        &mut self,
        tree: &RenderTree,
        render_state: &RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        let mut saved = FrameCache::default();
        saved.swap(self);
        let inner = std::mem::replace(&mut self.in_compositor_inner_call, false);
        let overlay = std::mem::replace(&mut self.overlay_frame_active, true);
        let invalidates = std::mem::replace(&mut self.overlay_invalidates_replay, false);
        let viewport = self.renderer.viewport_size();
        std::mem::swap(
            &mut self.backdrop_texture,
            &mut self.overlay_backdrop_texture,
        );

        let result =
            self.render_tree_frame(tree, Some(render_state), width, height, target, None, false);

        self.drain_all_layer_textures("overlay_finished");
        let active_flows = self.has_active_flows;
        saved.swap(self);
        self.has_active_flows |= active_flows;
        self.in_compositor_inner_call = inner;
        self.overlay_frame_active = overlay;
        let resources_changed =
            std::mem::replace(&mut self.overlay_invalidates_replay, invalidates);
        std::mem::swap(
            &mut self.backdrop_texture,
            &mut self.overlay_backdrop_texture,
        );
        self.renderer.resize(viewport.0, viewport.1);
        // A reused tree has new paint records; atlas changes can invalidate saved glyph UVs.
        let atlas_grew = self.text_ctx.take_atlas_grew();
        if self
            .cached_tree_scope
            .is_some_and(|(scope, _)| scope == tree.tree_scope_id())
            || atlas_grew
            || resources_changed
        {
            self.invalidate_render_cache_tagged("overlay_resources_changed");
        }
        result
    }
}
