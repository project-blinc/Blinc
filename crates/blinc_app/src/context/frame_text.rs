//! Prepare text streams and their cache snapshots for a motion frame.
use super::{
    GpuPrimitive, PrimitiveBatch, RenderContext, SvgElement, TextAlign, TextAlignment, TextAnchor,
    TextElement, TextVerticalAlign, cached_text, generate_text_decoration_primitives_by_layer,
    text_clip, to_gpu_generic_font,
};

pub(super) struct PreparedFrameText {
    pub(super) glyphs_by_layer: std::collections::BTreeMap<u32, cached_text::TextBatch>,
    pub(super) css_transformed_text_prims: PrimitiveBatch,
    pub(super) fg_glyphs: cached_text::TextBatch,
    pub(super) fg_decorations_by_layer: std::collections::HashMap<u32, Vec<GpuPrimitive>>,
}

/// The raster size for text shown `k` times larger than it is laid out: rounded up to a quarter
/// so the atlas does not fill with a new size every frame, 1 when it is not shown larger.
pub(super) fn quantize_raster(k: f32) -> f32 {
    if !k.is_finite() || k < 1.05 {
        return 1.0;
    }
    (k.min(4.0) * 4.0).ceil() / 4.0
}

/// How much an affine `[a, b, c, d, tx, ty]` scales along its stronger axis.
pub(super) fn affine_scale(affine: Option<[f32; 6]>) -> f32 {
    affine.map_or(1.0, |[a, b, c, d, ..]| a.hypot(b).max(c.hypot(d)))
}

/// The raster size motion text needs now: what its bound ancestors scale it by (or will, with
/// `by_target`) and what its own affine already does.
pub(super) fn needed_raster(
    tree: &blinc_layout::RenderTree,
    text: &TextElement,
    by_target: bool,
) -> f32 {
    raster_scale(tree, text.node, by_target).max(quantize_raster(affine_scale(text.css_affine)))
}

/// How much finer than its laid-out size the glyphs of a motion subtree's text are rasterized:
/// the scale its bound ancestors have, or with `by_target` will have when their springs arrive,
/// rounded up to a quarter so the atlas does not fill with a new size every frame. Layout stays
/// at the base size and the quads shrink back, so the text is placed where it always was and is
/// sharp when the card has grown to it. 1 when nothing scales it up.
pub(super) fn raster_scale(
    tree: &blinc_layout::RenderTree,
    node: super::LayoutNodeId,
    by_target: bool,
) -> f32 {
    let value = |v: &Option<blinc_animation::SharedAnimatedValue>, target: bool| -> f32 {
        v.as_ref()
            .and_then(|v| {
                v.lock()
                    .ok()
                    .map(|g| if target { g.target() } else { g.get() })
            })
            .unwrap_or(1.0)
    };
    let axis = |b: &blinc_layout::motion::MotionBindings, target: bool| -> f32 {
        value(&b.scale, target) * value(&b.scale_x, target).max(value(&b.scale_y, target))
    };
    let bindings = tree.motion_bindings_map();
    let (mut now, mut then) = (1.0f32, 1.0f32);
    for n in std::iter::once(node).chain(tree.layout_tree.ancestors(node)) {
        if let Some(b) = bindings.get(&n) {
            now *= axis(b, false);
            then *= axis(b, true);
        }
    }
    quantize_raster(if by_target { now.max(then) } else { now })
}

impl RenderContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_frame_text(
        &mut self,
        texts: &[TextElement],
        fg_texts: &[TextElement],
        motion_subtree_texts: &[TextElement],
        motion_subtree_svgs: &[SvgElement],
        raster_scales: &std::collections::HashMap<super::LayoutNodeId, f32>,
        viewport: (u32, u32),
        scale_factor: f32,
    ) -> PreparedFrameText {
        let (width, height) = viewport;
        // Prepare text glyphs with z_layer information
        // Store (z_layer, glyphs) to enable interleaved rendering
        let mut glyphs_by_layer: std::collections::BTreeMap<u32, cached_text::TextBatch> =
            std::collections::BTreeMap::new();
        let mut css_transformed_text_prims = PrimitiveBatch::new();
        for text in texts {
            // Skip text that's completely outside its clip bounds (visibility culling)
            // This prevents loading emoji fonts for off-screen text in scroll containers
            if let Some([clip_x, clip_y, clip_w, clip_h]) = text.clip_bounds {
                let text_right = text.x + text.width;
                let text_bottom = text.y + text.height;
                let clip_right = clip_x + clip_w;
                let clip_bottom = clip_y + clip_h;

                // Check if text is completely outside clip bounds
                if text.x >= clip_right
                    || text_right <= clip_x
                    || text.y >= clip_bottom
                    || text_bottom <= clip_y
                {
                    // Text is not visible, skip rendering entirely
                    continue;
                }
            }

            let alignment = match text.align {
                TextAlign::Left => TextAlignment::Left,
                TextAlign::Center => TextAlignment::Center,
                TextAlign::Right => TextAlignment::Right,
            };

            // Apply motion opacity to text color
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

            // Constrain wrapping to the available text or clip width.
            let effective_width = if let Some(clip) = text.clip_bounds {
                // Use clip width if it constrains the text
                clip[2].min(text.width)
            } else {
                text.width
            };

            // Wrap if effective width is significantly smaller than measured width
            let needs_wrap = text.wrap
                && effective_width < text.measured_width - cached_text::wrap_slack(scale_factor);

            // Always pass width for alignment - the layout engine needs max_width
            // to calculate center/right alignment offsets
            let wrap_width = Some(text.width);

            // Convert font family to GPU types
            let font_name = text.font_family.name.as_deref();
            let generic = to_gpu_generic_font(text.font_family.generic);
            let font_weight = text.weight.weight();

            // Map vertical alignment to text anchor
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

            // Render text shadow first (behind text) if present
            if let Some(shadow) = &text.text_shadow {
                let shadow_color = [
                    shadow.color.r,
                    shadow.color.g,
                    shadow.color.b,
                    shadow.color.a * text.motion_opacity,
                ];
                let shadow_x = text.x + shadow.offset_x * scale_factor;
                let shadow_y = y_pos + shadow.offset_y * scale_factor;
                if let Ok(shadow_glyphs) = self.text_ctx.prepare_text_with_style(
                    &text.content,
                    shadow_x,
                    shadow_y,
                    text.font_size,
                    shadow_color,
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
                    let mut owned = text_clip::prepare_glyphs(text, shadow_glyphs, (width, height));
                    if let Some(affine) = text.css_affine {
                        cached_text::convert_affine(&mut owned, affine, scale_factor, text.z_index);
                        css_transformed_text_prims.merge(owned);
                    } else {
                        glyphs_by_layer
                            .entry(text.z_index)
                            .or_default()
                            .append(owned);
                    }
                }
            }

            // Text a CSS transform scales up is rasterized at the size it is shown, laid out at the
            // base size.
            let k = quantize_raster(affine_scale(text.css_affine));
            match self
                .text_ctx
                .prepare_text_with_style(
                    &text.content,
                    text.x * k,
                    y_pos * k,
                    text.font_size * k,
                    color,
                    anchor,
                    alignment,
                    wrap_width.map(|w| w * k),
                    needs_wrap,
                    font_name,
                    generic,
                    font_weight,
                    text.italic,
                    layout_height.map(|h| h * k),
                    text.letter_spacing * k,
                )
                .map(|mut glyphs| {
                    if k != 1.0 {
                        for glyph in &mut glyphs {
                            for b in &mut glyph.bounds {
                                *b /= k;
                            }
                        }
                    }
                    glyphs
                }) {
                Ok(glyphs) => {
                    tracing::trace!(
                        "render_tree_with_motion: prepared {} glyphs for '{}' (font={:?})",
                        glyphs.len(),
                        text.content,
                        font_name
                    );
                    let mut owned = text_clip::prepare_glyphs(text, glyphs, (width, height));
                    if let Some(affine) = text.css_affine {
                        cached_text::convert_affine(&mut owned, affine, scale_factor, text.z_index);
                        css_transformed_text_prims.merge(owned);
                    } else {
                        // Snap glyph origins to physical pixels for sharp atlas sampling.
                        for glyph in &mut owned.glyphs {
                            glyph.bounds[0] = glyph.bounds[0].round();
                            glyph.bounds[1] = glyph.bounds[1].round();
                        }
                        glyphs_by_layer
                            .entry(text.z_index)
                            .or_default()
                            .append(owned);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "render_tree_with_motion: failed to prepare text '{}': {:?}",
                        text.content,
                        e
                    );
                }
            }
        }

        // Prepare foreground text glyphs (rendered after foreground primitives)
        let mut fg_glyphs = cached_text::TextBatch::default();
        for text in fg_texts {
            if let Some([clip_x, clip_y, clip_w, clip_h]) = text.clip_bounds {
                let text_right = text.x + text.width;
                let text_bottom = text.y + text.height;
                let clip_right = clip_x + clip_w;
                let clip_bottom = clip_y + clip_h;
                if text.x >= clip_right
                    || text_right <= clip_x
                    || text.y >= clip_bottom
                    || text_bottom <= clip_y
                {
                    continue;
                }
            }

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
            let needs_wrap = text.wrap
                && effective_width < text.measured_width - cached_text::wrap_slack(scale_factor);
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
                text.x,
                y_pos,
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
                let mut owned = text_clip::prepare_glyphs(text, glyphs, (width, height));
                if let Some(affine) = text.css_affine {
                    cached_text::convert_affine(&mut owned, affine, scale_factor, text.z_index);
                }
                fg_glyphs.append(owned);
            }
        }

        // Prepare motion glyphs as primitives with their live affine and owning clip rows.
        let mut motion_subtree_text_prims = cached_text::TextBatch::default();
        let mut motion_subtree_text_patch: Vec<(
            std::ops::Range<usize>,
            blinc_layout::tree::StableNodeId,
        )> = Vec::new();
        for (index, text) in motion_subtree_texts.iter().enumerate() {
            let patch_start = motion_subtree_text_prims.primitives.len();
            // Rasterize finer than the laid-out size when the card will be shown larger.
            let k = raster_scales
                .get(&text.node)
                .copied()
                .unwrap_or(1.0)
                .max(quantize_raster(affine_scale(text.css_affine)));
            if let Some(owned) = self.prepare_motion_text(text, k, scale_factor, (width, height)) {
                motion_subtree_text_prims.append(owned);
            }
            // Record the emitted range for the dispatch-time live
            // patch when this text sits under a composite-promoted
            // ancestor (its alpha was baked at BASE, factor excluded).
            let patch_end = motion_subtree_text_prims.primitives.len();
            if patch_end > patch_start {
                motion_subtree_text_prims.owners.push((
                    patch_start..patch_end,
                    text.node,
                    index,
                    k,
                ));
            }
            if patch_end > patch_start
                && let Some(ancestor) = text.composite_ancestor
            {
                motion_subtree_text_patch.push((patch_start..patch_end, ancestor));
            }
        }
        motion_subtree_text_prims.elements = std::sync::Arc::new(motion_subtree_texts.to_vec());

        // Cache prepared text for replay. Motion text remains separate from the static
        // texture.
        self.cached_glyphs_by_layer = Some(glyphs_by_layer.clone());
        self.cached_fg_glyphs = Some(fg_glyphs.clone());
        self.cached_motion_subtree_text_prims = Some(motion_subtree_text_prims.clone());
        self.motion_subtree_text_patch = motion_subtree_text_patch;
        self.cached_motion_subtree_svgs = Some(motion_subtree_svgs.to_vec());
        self.cached_css_transformed_text_prims = Some(css_transformed_text_prims.clone());

        // Prepare foreground text decorations once for the dispatch branches below.
        let fg_decorations_by_layer = generate_text_decoration_primitives_by_layer(fg_texts);

        tracing::trace!(
            "render_tree_with_motion: {} texts, {} fg texts, {} z-layers with glyphs, {} css-transformed",
            texts.len(),
            fg_texts.len(),
            glyphs_by_layer.len(),
            css_transformed_text_prims.primitives.len()
        );

        PreparedFrameText {
            glyphs_by_layer,
            css_transformed_text_prims,
            fg_glyphs,
            fg_decorations_by_layer,
        }
    }
}

impl RenderContext {
    /// The glyph primitives of one motion text, laid out at its base size and rasterized `k`
    /// times finer, with the quads shrunk back so it sits where it always did. `None` when it is
    /// outside its clip or cannot be prepared.
    pub(super) fn prepare_motion_text(
        &mut self,
        text: &TextElement,
        k: f32,
        scale_factor: f32,
        viewport: (u32, u32),
    ) -> Option<PrimitiveBatch> {
        let (width, height) = viewport;
        if let Some([clip_x, clip_y, clip_w, clip_h]) = text.clip_bounds {
            let text_right = text.x + text.width;
            let text_bottom = text.y + text.height;
            let clip_right = clip_x + clip_w;
            let clip_bottom = clip_y + clip_h;
            if text.x >= clip_right
                || text_right <= clip_x
                || text.y >= clip_bottom
                || text_bottom <= clip_y
            {
                return None;
            }
        }

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
        let needs_wrap = text.wrap
            && effective_width < text.measured_width - cached_text::wrap_slack(scale_factor);
        let wrap_width = Some(text.width);
        let font_name = text.font_family.name.as_deref();
        let generic = to_gpu_generic_font(text.font_family.generic);
        let font_weight = text.weight.weight();

        let (anchor, y_pos, use_layout_height) = match text.v_align {
            TextVerticalAlign::Center => (TextAnchor::Center, text.y + text.height / 2.0, false),
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

        let mut glyphs = self
            .text_ctx
            .prepare_text_with_style(
                &text.content,
                text.x * k,
                y_pos * k,
                text.font_size * k,
                color,
                anchor,
                alignment,
                wrap_width.map(|w| w * k),
                needs_wrap,
                font_name,
                generic,
                font_weight,
                text.italic,
                layout_height.map(|h| h * k),
                text.letter_spacing * k,
            )
            .ok()?;
        if k != 1.0 {
            for glyph in &mut glyphs {
                for b in &mut glyph.bounds {
                    *b /= k;
                }
            }
        }
        let mut owned = text_clip::prepare_glyphs(text, glyphs, (width, height));
        cached_text::convert_affine(
            &mut owned,
            text.css_affine.unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
            scale_factor,
            text.z_index,
        );
        Some(owned)
    }
}

impl RenderContext {
    /// Prepare again, at the size they now need, the glyphs of motion text whose springs are
    /// heading for a different scale than they were rasterized for, in place: each keeps its slot and the pose it was
    /// painted at, so only its bitmap changes and nothing else is repainted.
    pub(super) fn refine_motion_text(&mut self, tree: &blinc_layout::RenderTree, dpi: f32) {
        let Some(mut text) = self.cached_motion_subtree_text_prims.take() else {
            return;
        };
        if text.aux_data.is_empty() {
            let viewport = self.renderer.viewport_size();
            for i in 0..text.owners.len() {
                let (range, node, element, k) = text.owners[i].clone();
                let Some(source) = text.elements.get(element).cloned() else {
                    continue;
                };
                let needed = needed_raster(tree, &source, true);
                if (needed - k).abs() < 1e-3 {
                    continue;
                }
                let Some(fresh) = self.prepare_motion_text(&source, needed, dpi, viewport) else {
                    continue;
                };
                if fresh.primitives.len() != range.len() {
                    continue;
                }
                for (old, new) in text.primitives[range].iter_mut().zip(fresh.primitives) {
                    // The slot and the clip lane stay as they were given.
                    let lane = old.node_slot;
                    *old = new;
                    old.node_slot = lane;
                }
                text.owners[i].3 = needed;
            }
        }
        self.cached_motion_subtree_text_prims = Some(text);
    }
}
