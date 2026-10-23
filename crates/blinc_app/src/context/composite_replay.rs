//! Bake and replay promoted subtrees and 3D layer content.
use super::{
    Color, ImageElement, LayoutNodeId, PrimitiveBatch, RenderContext, RenderTree, SvgElement,
    TextAlign, TextAlignment, TextAnchor, TextElement, TextVerticalAlign, Transform3DLayerInfo,
    cached_text, composite_scratch, text_clip, to_gpu_generic_font,
};

impl RenderContext {
    /// Fingerprint intrinsic subtree content and group operations for bake reuse.
    pub(super) fn hash_composite_scratch(batch: &blinc_gpu::primitives::PrimitiveBatch) -> u64 {
        composite_scratch::hash(batch)
    }

    /// Blit promoted CSS textures using live animation values and captured ancestor clips.
    pub(super) fn composite_css_layers_overlay(
        &mut self,
        tree: &blinc_layout::RenderTree,
        target_view: &wgpu::TextureView,
    ) {
        use slotmap::Key as _;

        // Collect (key, screen_aabb, natural_size, stable_id) up-front
        // so we don't hold the `dynamic_regions` borrow across the
        // `&mut self.renderer` blit calls.
        struct CompositeJob {
            key: u64,
            screen_aabb: [f32; 4],
            natural_size: (u32, u32),
            stable_id: Option<blinc_layout::tree::StableNodeId>,
            ambient_clip: Option<([f32; 4], [f32; 4])>,
            visit_seq: u32,
        }
        let mut jobs: Vec<CompositeJob> = {
            let regions = tree.dynamic_regions();
            regions
                .iter()
                .filter_map(|(node, region)| match region.kind {
                    blinc_layout::renderer::DynamicKind::CssAnimated {
                        natural_size,
                        ambient_clip,
                    } if natural_size.0 > 0 && natural_size.1 > 0 => Some(CompositeJob {
                        key: node.data().as_ffi(),
                        screen_aabb: region.screen_aabb,
                        natural_size,
                        stable_id: tree.stable_id(*node),
                        ambient_clip,
                        visit_seq: region.visit_seq,
                    }),
                    _ => None,
                })
                .collect()
        };
        // HashMap iteration is non-deterministic; sort by walker
        // visit order so overlapping subtrees composite back-to-front
        // (matches tree paint order). z_layer alone is insufficient
        // because flex siblings share the same z_layer.
        jobs.sort_by_key(|j| j.visit_seq);
        if jobs.is_empty() {
            return;
        }
        // Snapshot current animated properties per stable id under a
        // single lock acquisition. We need: opacity / translate / scale
        // to apply to the blit.
        let mut anim_props: std::collections::HashMap<
            blinc_layout::tree::StableNodeId,
            blinc_animation::KeyframeProperties,
        > = std::collections::HashMap::new();
        if let Ok(store) = tree.css_anim_store().lock() {
            for job in &jobs {
                let Some(sid) = job.stable_id else { continue };
                if let Some(anim) = store.animations.get(&sid) {
                    if anim.is_playing {
                        anim_props.insert(sid, anim.current_properties.clone());
                        continue;
                    }
                }
                if let Some(trans) = store.transitions.get(&sid) {
                    if trans.is_playing {
                        anim_props.insert(sid, trans.current_properties.clone());
                    }
                }
            }
        }
        // Apply live translation, scale and opacity to the base-state texture. The blit
        // does not apply 2D rotation.
        for job in jobs {
            let Some((texture, content_size)) = self.css_composited_textures.get(&job.key) else {
                continue;
            };
            let allocated_size = *content_size;
            let props = job
                .stable_id
                .and_then(|sid| anim_props.get(&sid))
                .cloned()
                .unwrap_or_default();
            let translate = (
                props.translate_x.unwrap_or(0.0),
                props.translate_y.unwrap_or(0.0),
            );
            let scale = (props.scale_x.unwrap_or(1.0), props.scale_y.unwrap_or(1.0));
            let opacity = props.opacity.unwrap_or(1.0);

            let dpi = tree.scale_factor().max(1.0);
            let dest_pos = (
                job.screen_aabb[0] + translate.0 * dpi,
                job.screen_aabb[1] + translate.1 * dpi,
            );
            let dest_size = (
                job.natural_size.0 as f32 * scale.0,
                job.natural_size.1 as f32 * scale.1,
            );

            // Restore the captured ancestor scissor and rounded clip radius for the blit.
            let blit_clip = job.ambient_clip;

            // Use the allocated texture extent for source UVs, including any pool padding.
            self.renderer.blit_tight_texture_to_target(
                &texture.view,
                allocated_size,
                target_view,
                dest_pos,
                dest_size,
                opacity,
                blinc_core::BlendMode::Normal,
                blit_clip,
                None,
                0.0,
            );
        }
    }

    /// Render motion text and SVG after their owning backgrounds.
    pub(super) fn dispatch_motion_subtree_overlay_pools(
        &mut self,
        tree: &blinc_layout::RenderTree,
        target_view: &wgpu::TextureView,
    ) {
        // TEXT — alpha + blit-anchored center remap.
        if let Some(mut text) = self.cached_motion_subtree_text_prims.clone() {
            if !text.is_empty() {
                if !self.motion_subtree_text_patch.is_empty() {
                    let dpi = tree.scale_factor().max(1.0);
                    if let Ok(store) = tree.css_anim_store().lock() {
                        for (range, sid) in &self.motion_subtree_text_patch {
                            let props = store
                                .animations
                                .get(sid)
                                .filter(|a| a.is_playing)
                                .map(|a| &a.current_properties)
                                .or_else(|| {
                                    store
                                        .transitions
                                        .get(sid)
                                        .filter(|t| t.is_playing)
                                        .map(|t| &t.current_properties)
                                });
                            let Some(props) = props else { continue };
                            let live_opacity = props.opacity.unwrap_or(1.0);
                            let t1 = (
                                props.translate_x.unwrap_or(0.0),
                                props.translate_y.unwrap_or(0.0),
                            );
                            let s1 = (props.scale_x.unwrap_or(1.0), props.scale_y.unwrap_or(1.0));
                            let baseline = self.composite_patch_baselines.get(sid).copied();
                            for prim in &mut text.primitives[range.clone()] {
                                prim.color[3] *= live_opacity;
                                if let Some((aabb, t0, s0)) = baseline {
                                    // p_base = aabb + (p0 - aabb - t0*dpi)/s0
                                    // p1     = aabb + t1*dpi + p_base_rel*s1
                                    let cx = prim.bounds[0] + prim.bounds[2] * 0.5;
                                    let cy = prim.bounds[1] + prim.bounds[3] * 0.5;
                                    let rel_x = (cx - aabb[0] - t0.0 * dpi) / s0.0.max(1e-6);
                                    let rel_y = (cy - aabb[1] - t0.1 * dpi) / s0.1.max(1e-6);
                                    let nx = aabb[0] + t1.0 * dpi + rel_x * s1.0;
                                    let ny = aabb[1] + t1.1 * dpi + rel_y * s1.1;
                                    prim.bounds[0] = nx - prim.bounds[2] * 0.5;
                                    prim.bounds[1] = ny - prim.bounds[3] * 0.5;
                                }
                            }
                        }
                    }
                }
                self.render_text(target_view, &text);
            }
        }
        // SVG — alpha only (icons are small; the settle repaint lands
        // the exact final position when the animation completes).
        if let Some(mut svgs) = self.cached_motion_subtree_svgs.clone() {
            if !svgs.is_empty() {
                if svgs.iter().any(|s| s.composite_ancestor.is_some()) {
                    if let Ok(store) = tree.css_anim_store().lock() {
                        for svg in &mut svgs {
                            let Some(sid) = svg.composite_ancestor else {
                                continue;
                            };
                            let props = store
                                .animations
                                .get(&sid)
                                .filter(|a| a.is_playing)
                                .map(|a| &a.current_properties)
                                .or_else(|| {
                                    store
                                        .transitions
                                        .get(&sid)
                                        .filter(|t| t.is_playing)
                                        .map(|t| &t.current_properties)
                                });
                            let Some(props) = props else { continue };
                            svg.motion_opacity *= props.opacity.unwrap_or(1.0);
                        }
                    }
                }
                let dpi = tree.scale_factor();
                self.render_rasterized_svgs(target_view, &svgs, dpi);
            }
        }
    }

    /// Blit promoted motion textures in tree paint order with live transforms and clips.
    pub(super) fn composite_motion_layers_overlay(
        &mut self,
        tree: &blinc_layout::RenderTree,
        target_view: &wgpu::TextureView,
    ) {
        use slotmap::Key as _;

        struct MotionCompositeJob {
            key: u64,
            node: blinc_layout::tree::LayoutNodeId,
            screen_aabb: [f32; 4],
            natural_size: (u32, u32),
            ambient_clip: Option<([f32; 4], [f32; 4])>,
            visit_seq: u32,
        }
        let mut jobs: Vec<MotionCompositeJob> = {
            let regions = tree.dynamic_regions();
            regions
                .iter()
                .filter_map(|(node, region)| match region.kind {
                    blinc_layout::renderer::DynamicKind::MotionSubtreeTexture {
                        natural_size,
                        ambient_clip,
                    } if natural_size.0 > 0 && natural_size.1 > 0 => Some(MotionCompositeJob {
                        key: node.data().as_ffi(),
                        node: *node,
                        screen_aabb: region.screen_aabb,
                        natural_size,
                        ambient_clip,
                        visit_seq: region.visit_seq,
                    }),
                    _ => None,
                })
                .collect()
        };
        // Sort by walker visit order so equal-z siblings composite in tree paint order.
        jobs.sort_by_key(|j| j.visit_seq);
        if jobs.is_empty() {
            return;
        }
        let bindings_map = tree.motion_bindings_map().clone();
        for job in jobs {
            let Some((texture, content_size)) = self.motion_subtree_textures.get(&job.key) else {
                continue;
            };
            let allocated_size = *content_size;

            // Default to identity if the node lost its bindings
            // between paint and overlay (shouldn't happen — bindings
            // are stable across the frame — but stay defensive so a
            // missing entry produces a static blit rather than panic).
            let (translate, scale, opacity, rotate_deg) =
                if let Some(bindings) = bindings_map.get(&job.node) {
                    let translate = match bindings.get_transform() {
                        Some(blinc_core::Transform::Affine2D(a)) => (a.elements[4], a.elements[5]),
                        _ => (0.0, 0.0),
                    };
                    let scale = bindings.get_scale().unwrap_or((1.0, 1.0));
                    let opacity = bindings.get_opacity().unwrap_or(1.0);
                    // Rotation is in DEGREES from MotionBindings
                    // (covers both spring-based `rotation` and
                    // continuous `rotation_timeline`). Shader takes
                    // sin/cos radians, so convert at the blit site.
                    let rotate = bindings.get_rotation().unwrap_or(0.0);
                    (translate, scale, opacity, rotate)
                } else {
                    ((0.0, 0.0), (1.0, 1.0), 1.0, 0.0)
                };
            let rotate_z_rad = rotate_deg.to_radians();

            let dpi = tree.scale_factor().max(1.0);
            let dest_pos = (
                job.screen_aabb[0] + translate.0 * dpi,
                job.screen_aabb[1] + translate.1 * dpi,
            );
            let dest_size = (
                job.natural_size.0 as f32 * scale.0,
                job.natural_size.1 as f32 * scale.1,
            );

            // Apply the captured ancestor scissor and rounded outline.
            let blit_clip = job.ambient_clip;

            // Use the allocated texture extent for source UVs, including pool padding.
            self.renderer.blit_tight_texture_to_target(
                &texture.view,
                allocated_size,
                target_view,
                dest_pos,
                dest_size,
                opacity,
                blinc_core::BlendMode::Normal,
                blit_clip,
                None,
                rotate_z_rad,
            );
        }
    }

    /// Draw collected text, SVGs and images into their owning 3D layer textures.
    pub(super) fn render_3d_layer_elements(
        &mut self,
        target: &wgpu::TextureView,
        info: &Transform3DLayerInfo,
        texts: &[TextElement],
        svgs: &[SvgElement],
        images: &[ImageElement],
        scale_factor: f32,
    ) {
        let [lx, ly, lw, lh] = info.layer_bounds;
        if lw <= 0.0 || lh <= 0.0 {
            return;
        }

        let tex_w = (lw.ceil() as u32).max(1);
        let tex_h = (lh.ceil() as u32).max(1);

        // Acquire offscreen texture
        let layer_tex = self.renderer.acquire_layer_texture((tex_w, tex_h), false);
        self.renderer
            .clear_target(&layer_tex.view, wgpu::Color::TRANSPARENT);

        // Set viewport to offscreen texture size
        self.renderer.set_viewport_override((tex_w, tex_h));

        // Render offset text glyphs
        if !texts.is_empty() {
            let mut text_runs = vec![cached_text::TextBatch::default()];
            for text in texts {
                let alignment = match text.align {
                    TextAlign::Left => TextAlignment::Left,
                    TextAlign::Center => TextAlignment::Center,
                    TextAlign::Right => TextAlignment::Right,
                };

                let color = if text.motion_opacity < 1.0 {
                    [
                        text.color[0],
                        text.color[1],
                        text.color[2],
                        text.color[3] * text.motion_opacity,
                    ]
                } else {
                    text.color
                };

                let effective_width = if let Some(clip) = text.clip_bounds {
                    clip[2].min(text.width)
                } else {
                    text.width
                };
                let needs_wrap = text.wrap && effective_width < text.measured_width - 2.0;
                let wrap_width = Some(text.width);
                let font_name = text.font_family.name.as_deref();
                let generic = to_gpu_generic_font(text.font_family.generic);
                let font_weight = text.weight.weight();

                let (anchor, y_pos, use_layout_height) = match text.v_align {
                    TextVerticalAlign::Center => {
                        (TextAnchor::Center, text.y + text.height / 2.0, false)
                    }
                    TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                    TextVerticalAlign::Baseline => {
                        let baseline_y = text.y + text.half_leading + text.ascender;
                        (TextAnchor::Baseline, baseline_y, false)
                    }
                };
                let layout_height = if use_layout_height {
                    Some(text.height)
                } else {
                    None
                };

                if let Ok(glyphs) = self.text_ctx.prepare_text_with_style(
                    &text.content,
                    text.x - lx,
                    y_pos - ly,
                    text.font_size,
                    color,
                    anchor,
                    alignment,
                    wrap_width,
                    needs_wrap,
                    font_name,
                    generic,
                    font_weight,
                    text.italic,
                    layout_height,
                    text.letter_spacing,
                ) {
                    let owned = text_clip::prepare_layer_glyphs(
                        text,
                        glyphs,
                        (tex_w, tex_h),
                        info,
                        scale_factor,
                    );
                    // A run uses one pipeline. Flush at plain/affine boundaries
                    // so overlapping text retains its collected paint order.
                    let previous = text_runs.last().unwrap();
                    if (!owned.glyphs.is_empty() && !previous.primitives.is_empty())
                        || (!owned.primitives.is_empty() && !previous.glyphs.is_empty())
                    {
                        text_runs.push(cached_text::TextBatch::default());
                    }
                    text_runs.last_mut().unwrap().append(owned);
                }
            }

            for run in &text_runs {
                self.render_text(&layer_tex.view, run);
            }
        }

        // Render offset images (mutate in place — we own these from partition)
        if !images.is_empty() {
            let mut offset_images = images.to_vec();
            for img in &mut offset_images {
                img.x -= lx;
                img.y -= ly;
                if let Some(ref mut clip) = img.clip_bounds {
                    clip[0] -= lx;
                    clip[1] -= ly;
                }
                if let Some(ref mut scroll) = img.scroll_clip {
                    scroll[0] -= lx;
                    scroll[1] -= ly;
                }
            }
            self.render_images(&layer_tex.view, &offset_images, lw, lh, scale_factor);
        }

        // Render offset SVGs (mutate in place — we own these from partition)
        if !svgs.is_empty() {
            let mut offset_svgs = svgs.to_vec();
            for svg in &mut offset_svgs {
                svg.x -= lx;
                svg.y -= ly;
                if let Some(ref mut clip) = svg.clip_bounds {
                    clip[0] -= lx;
                    clip[1] -= ly;
                }
            }
            self.render_rasterized_svgs(&layer_tex.view, &offset_svgs, scale_factor);
        }

        // Restore viewport
        self.renderer.restore_viewport();

        // Blit with perspective transform
        self.renderer.blit_tight_texture_to_target(
            &layer_tex.view,
            (tex_w, tex_h),
            target,
            (lx, ly),
            (lw, lh),
            info.opacity,
            blinc_core::BlendMode::Normal,
            None,
            Some(info.transform_3d),
            0.0,
        );

        self.renderer.release_layer_texture(layer_tex);
    }
}
