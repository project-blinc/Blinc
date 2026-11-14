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

/// The raster size for text shown `k` times larger than it is laid out, 1 when it is not shown
/// larger. In flight it is rounded up to a quarter so the atlas does not fill with a new size
/// every frame; at rest it is the exact scale, so the glyphs land 1:1 on the display.
pub(super) fn raster_size(k: f32, moving: bool) -> f32 {
    if !k.is_finite() {
        return 1.0;
    }
    // A node at rest shown slightly smaller (a pressed button) is rasterized at that size too.
    if !moving && (0.5..1.05).contains(&k) {
        return (k * 100.0).round() / 100.0;
    }
    if k < 1.05 {
        return 1.0;
    }
    let k = k.min(4.0);
    if moving {
        (k * 4.0).ceil() / 4.0
    } else {
        (k * 100.0).round() / 100.0
    }
}

/// Whether an affine only moves and scales, so glyphs laid out on the pixel grid stay on it when
/// their origins are snapped to whole device pixels.
pub(super) fn is_axis_aligned(affine: Option<[f32; 6]>) -> bool {
    affine.is_none_or(|[a, b, c, d, ..]| b.abs() < 1e-4 && c.abs() < 1e-4 && (a - d).abs() < 1e-3)
}

/// A coordinate rounded to a whole device pixel. A value on the half pixel goes the same way
/// however much float noise the path that made it carries.
pub(super) fn snap_px(v: f32) -> f32 {
    (v - 1e-3).round()
}

/// Put each glyph in `batch` on the display grid, as static text is, for text whose transform
/// keeps it axis-aligned: the scale in its local affine is folded into its quad, so the quad is
/// the size of its bitmap and drawn with no transform, and the quad's top-left is rounded to a
/// whole device pixel.
pub(super) fn snap_to_pixels(batch: &mut PrimitiveBatch) {
    for p in &mut batch.primitives {
        let [x, y, w, h] = p.bounds;
        let (sx, sy) = (p.local_affine[0], p.local_affine[3]);
        let (fw, fh) = (w * sx.abs(), h * sy.abs());
        let (left, top) = (x + w * 0.5 - fw * 0.5, y + h * 0.5 - fh * 0.5);
        p.bounds = [snap_px(left), snap_px(top), fw, fh];
        p.local_affine = [1.0, 0.0, 0.0, 1.0];
    }
}

/// How much an affine `[a, b, c, d, tx, ty]` scales along its stronger axis.
pub(super) fn affine_scale(affine: Option<[f32; 6]>) -> f32 {
    affine.map_or(1.0, |[a, b, c, d, ..]| a.hypot(b).max(c.hypot(d)))
}

/// Whether `node` is inside a scroll container whose content is moved by rows. Its glyphs move
/// by whole device pixels with the rows and are never baked: a settle bake would leave them
/// behind the next time it scrolls.
pub(super) fn in_scroll_rows(tree: &blinc_layout::RenderTree, node: super::LayoutNodeId) -> bool {
    std::iter::once(node)
        .chain(tree.layout_tree.ancestors(node))
        .any(|n| tree.is_scroll_row_node(n))
}

/// Whether a bound node at or above `node` is moving.
pub(super) fn chain_moving(tree: &blinc_layout::RenderTree, node: super::LayoutNodeId) -> bool {
    let bindings = tree.motion_bindings_map();
    std::iter::once(node)
        .chain(tree.layout_tree.ancestors(node))
        .any(|n| bindings.get(&n).is_some_and(|b| b.is_any_animating()))
}

/// The raster size motion text needs now: what its bound ancestors scale it by (or will, with
/// `by_target`) and what its own affine already does.
pub(super) fn needed_raster(
    tree: &blinc_layout::RenderTree,
    text: &TextElement,
    by_target: bool,
) -> f32 {
    let (scale, moving) = raster_scale(tree, text.node, by_target);
    raster_size(scale.max(affine_scale(text.css_affine)), moving)
}

/// The raster size for text in a group that the node table does not move, and whether it is in
/// flight: what its own affine scales it by, which is the scale of a CSS transform painted into
/// the frame. While a CSS animation of the scale plays on it or an ancestor the size is rounded up
/// to a quarter, as motion text's is; an animation of anything else leaves it as at rest.
pub(super) fn grouped_raster(tree: &blinc_layout::RenderTree, text: &TextElement) -> (f32, bool) {
    let scaling = std::iter::once(text.node)
        .chain(tree.layout_tree.ancestors(text.node))
        .any(|n| {
            tree.has_css_animation(n)
                && tree
                    .get_css_animation_properties(n)
                    .is_some_and(|p| p.scale_x.is_some() || p.scale_y.is_some())
        });
    let moving = scaling || chain_moving(tree, text.node);
    (raster_size(affine_scale(text.css_affine), moving), moving)
}

/// How much its bound ancestors scale the text of `node`, now or (with `by_target`, while they
/// move) when their springs arrive, and whether they are moving. Layout stays at the base size
/// and the glyphs are rasterized finer, with the quads shrunk back, so the text is placed where
/// it always was and is sharp when the card has grown to it.
pub(super) fn raster_scale(
    tree: &blinc_layout::RenderTree,
    node: super::LayoutNodeId,
    by_target: bool,
) -> (f32, bool) {
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
    let moving = chain_moving(tree, node);
    (
        if moving && by_target {
            now.max(then)
        } else {
            now
        },
        moving,
    )
}

impl RenderContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_frame_text(
        &mut self,
        texts: &[TextElement],
        fg_texts: &[TextElement],
        motion_subtree_texts: &[TextElement],
        motion_subtree_svgs: &[SvgElement],
        raster_scales: &std::collections::HashMap<super::LayoutNodeId, (f32, bool)>,
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
            let k = raster_size(affine_scale(text.css_affine), false);
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
                        if is_axis_aligned(Some(affine)) {
                            snap_to_pixels(&mut owned);
                        }
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
            let (binding_scale, moving) = raster_scales
                .get(&text.node)
                .copied()
                .unwrap_or((1.0, false));
            let k = raster_size(binding_scale.max(affine_scale(text.css_affine)), moving);
            // Text snaps to the display grid at rest and moves smoothly with its node.
            let snap = !moving;
            if let Some(owned) =
                self.prepare_motion_text(text, k, scale_factor, (width, height), snap)
            {
                motion_subtree_text_prims.append(owned);
            }
            // Record the emitted range for the dispatch-time live
            // patch when this text sits under a composite-promoted
            // ancestor (its alpha was baked at BASE, factor excluded).
            let patch_end = motion_subtree_text_prims.primitives.len();
            if patch_end > patch_start {
                motion_subtree_text_prims.owners.push(cached_text::TextRun {
                    range: patch_start..patch_end,
                    node: text.node,
                    element: index,
                    k,
                    snapped: snap,
                    baked: None,
                });
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
        snap: bool,
    ) -> Option<PrimitiveBatch> {
        let (width, height) = viewport;
        if let Some([clip_x, clip_y, clip_w, clip_h]) = text.clip_bounds
            && !text.in_scroll_window
        {
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
        if snap && is_axis_aligned(text.css_affine) {
            snap_to_pixels(&mut owned);
        }
        Some(owned)
    }
}

impl RenderContext {
    /// Keep the glyphs of motion text sharp as their nodes move and when they stop, in place:
    /// each run keeps its slot and the pose it was painted at while its node moves, and only its
    /// bitmap is prepared again when the scale its springs are heading for changes. When the
    /// node stops, the run is prepared at the exact scale it ended at, placed through the rows
    /// on the CPU and snapped to whole device pixels, and drawn with no row, so it sits on the
    /// display grid as static text does. When the node moves again the run returns to its
    /// painted pose and its slot.
    pub(super) fn refine_motion_text(&mut self, tree: &blinc_layout::RenderTree, dpi: f32) {
        let Some(mut text) = self.cached_motion_subtree_text_prims.take() else {
            return;
        };
        if text.aux_data.is_empty() && text.glyphs.is_empty() {
            let viewport = self.renderer.viewport_size();
            let bindings = tree.motion_bindings_map();
            let ancestors = |n: super::LayoutNodeId| tree.layout_tree.ancestors(n);
            for i in 0..text.owners.len() {
                let run = text.owners[i].clone();
                let Some(source) = text.elements.get(run.element).cloned() else {
                    continue;
                };
                let Some(lane) = text.primitives.get(run.range.start).map(|p| p.node_slot) else {
                    continue;
                };
                if in_scroll_rows(tree, run.node) {
                    continue;
                }
                let moving = chain_moving(tree, run.node);
                match (moving, run.baked) {
                    // Moving again: back to the painted pose, at the size it now needs.
                    (true, Some(slot_lane)) => {
                        let needed = needed_raster(tree, &source, true);
                        if let Some(fresh) =
                            self.prepare_motion_text(&source, needed, dpi, viewport, false)
                            && fresh.primitives.len() == run.range.len()
                        {
                            for (old, new) in text.primitives[run.range.clone()]
                                .iter_mut()
                                .zip(fresh.primitives)
                            {
                                *old = new;
                                old.node_slot = slot_lane;
                            }
                            text.owners[i].k = needed;
                            text.owners[i].snapped = false;
                            text.owners[i].baked = None;
                        }
                    }
                    // Moving: the bitmap follows the scale it is heading for.
                    (true, None) => {
                        let needed = needed_raster(tree, &source, true);
                        if (needed - run.k).abs() < 1e-3 && !run.snapped {
                            continue;
                        }
                        // Unsnapped, so it moves smoothly with the card.
                        let Some(fresh) =
                            self.prepare_motion_text(&source, needed, dpi, viewport, false)
                        else {
                            continue;
                        };
                        if fresh.primitives.len() != run.range.len() {
                            continue;
                        }
                        for (old, new) in text.primitives[run.range.clone()]
                            .iter_mut()
                            .zip(fresh.primitives)
                        {
                            // The slot and the clip lane stay as they were given.
                            *old = new;
                            old.node_slot = lane;
                        }
                        text.owners[i].k = needed;
                        text.owners[i].snapped = false;
                    }
                    // Stopped: bake the pose it ended at.
                    (false, None) => {
                        let Some(row) = self.node_rows.text_row(run.node, &ancestors) else {
                            continue;
                        };
                        if !is_axis_aligned(source.css_affine) {
                            continue;
                        }
                        if self.node_rows.chain_is_rest(row) {
                            // Back where it was painted: onto the display grid again.
                            if !run.snapped {
                                let k = ((affine_scale(source.css_affine)).clamp(0.25, 4.0)
                                    * 100.0)
                                    .round()
                                    / 100.0;
                                if let Some(fresh) =
                                    self.prepare_motion_text(&source, k, dpi, viewport, true)
                                    && fresh.primitives.len() == run.range.len()
                                {
                                    for (old, new) in text.primitives[run.range.clone()]
                                        .iter_mut()
                                        .zip(fresh.primitives)
                                    {
                                        *old = new;
                                        old.node_slot = lane;
                                    }
                                    text.owners[i].k = k;
                                    text.owners[i].snapped = true;
                                }
                            }
                            continue;
                        }
                        let Some(scale) = self.node_rows.uniform_scale(row) else {
                            continue;
                        };
                        let k = ((affine_scale(source.css_affine) * scale).clamp(0.25, 4.0)
                            * 100.0)
                            .round()
                            / 100.0;
                        let Some(fresh) =
                            self.prepare_motion_text(&source, k, dpi, viewport, false)
                        else {
                            continue;
                        };
                        if fresh.primitives.len() != run.range.len() {
                            continue;
                        }
                        let rows = self.node_rows.rows();
                        for (old, new) in text.primitives[run.range.clone()]
                            .iter_mut()
                            .zip(fresh.primitives)
                        {
                            let mut glyph = new;
                            glyph.node_slot = lane;
                            let mut placed = blinc_gpu::node_table::resolve_primitive(rows, &glyph);
                            placed.bounds[0] = snap_px(placed.bounds[0]);
                            placed.bounds[1] = snap_px(placed.bounds[1]);
                            *old = placed;
                        }
                        text.owners[i].k = k;
                        text.owners[i].baked = Some(lane);
                    }
                    (false, Some(_)) => {}
                }
            }
        }
        self.cached_motion_subtree_text_prims = Some(text);
    }
}

impl RenderContext {
    /// Prepare the text of `run` again at `k` and `snap` and put its glyphs over `indices` in
    /// `batch`, with the node slot lanes `lane`, if it comes out the same size. With `bake` the
    /// glyphs are placed through the node table's rows on the CPU, onto the display grid, and
    /// carry no slot.
    #[allow(clippy::too_many_arguments)]
    fn place_captured(
        &mut self,
        batch: &mut blinc_gpu::PrimitiveBatch,
        indices: &[usize],
        run: &cached_text::CapturedRun,
        k: f32,
        snap: bool,
        lane: [f32; 4],
        bake: bool,
        dpi: f32,
    ) -> bool {
        let viewport = self.renderer.viewport_size();
        let fresh = self.prepare_captured_text_at(&run.element, viewport, dpi, k, Some(snap));
        if fresh.primitives.len() != indices.len() {
            return false;
        }
        let rows = bake.then(|| self.node_rows.rows().to_vec());
        for (&i, new) in indices.iter().zip(fresh.primitives) {
            let mut glyph = new;
            glyph.node_slot = lane;
            batch.primitives[i] = match &rows {
                Some(rows) => {
                    let mut placed = blinc_gpu::node_table::resolve_primitive(rows, &glyph);
                    placed.bounds[0] = snap_px(placed.bounds[0]);
                    placed.bounds[1] = snap_px(placed.bounds[1]);
                    placed
                }
                None => glyph,
            };
        }
        true
    }

    /// What [`refine_motion_text`](Self::refine_motion_text) does for the texts painted inside
    /// groups, whose glyphs sit in the dynamic batch among the group's other content: the same
    /// three states, found by the run id each glyph carries.
    pub(super) fn refine_captured_text(&mut self, tree: &blinc_layout::RenderTree, dpi: f32) {
        if self.captured_runs.is_empty() {
            return;
        }
        let Some(mut batch) = self.cached_dynamic_batch.take() else {
            return;
        };
        let ancestors = |n: super::LayoutNodeId| tree.layout_tree.ancestors(n);
        let mut by_id: std::collections::HashMap<u32, Vec<usize>> =
            std::collections::HashMap::new();
        for (i, p) in batch.primitives.iter().enumerate() {
            let id = p.node_slot[3] as u32;
            if id > 0 {
                by_id.entry(id).or_default().push(i);
            }
        }
        let mut runs = std::mem::take(&mut self.captured_runs);
        for run in &mut runs {
            let Some(indices) = by_id.get(&run.id) else {
                continue;
            };
            let moving = chain_moving(tree, run.element.node);
            let lane = batch.primitives[indices[0]].node_slot;
            match (moving, run.baked) {
                // Moving again: back to the painted pose, at the size it now needs.
                (true, Some(saved)) => {
                    let needed = needed_raster(tree, &run.element, true);
                    let snapshot = run.clone();
                    if self.place_captured(
                        &mut batch, indices, &snapshot, needed, false, saved, false, dpi,
                    ) {
                        run.k = needed;
                        run.snapped = false;
                        run.baked = None;
                    }
                }
                // Moving: smooth, and the bitmap follows the scale it is heading for.
                (true, None) => {
                    let needed = needed_raster(tree, &run.element, true);
                    if (needed - run.k).abs() < 1e-3 && !run.snapped {
                        continue;
                    }
                    let snapshot = run.clone();
                    if self.place_captured(
                        &mut batch, indices, &snapshot, needed, false, lane, false, dpi,
                    ) {
                        run.k = needed;
                        run.snapped = false;
                    }
                }
                // Stopped: bake the pose it ended at, or back onto the grid if it is where it began.
                (false, None) => {
                    let Some(row) = self
                        .node_rows
                        .text_row(run.element.node, &ancestors)
                        .map(|r| r - 1)
                    else {
                        continue;
                    };
                    if !is_axis_aligned(run.element.css_affine) {
                        continue;
                    }
                    let snapshot = run.clone();
                    if self.node_rows.chain_is_rest(row) {
                        if !run.snapped {
                            let k = raster_size(affine_scale(run.element.css_affine), false);
                            if self.place_captured(
                                &mut batch, indices, &snapshot, k, true, lane, false, dpi,
                            ) {
                                run.k = k;
                                run.snapped = true;
                            }
                        }
                        continue;
                    }
                    let Some(scale) = self.node_rows.uniform_scale(row) else {
                        continue;
                    };
                    let k = ((affine_scale(run.element.css_affine) * scale).clamp(0.25, 4.0)
                        * 100.0)
                        .round()
                        / 100.0;
                    if self
                        .place_captured(&mut batch, indices, &snapshot, k, false, lane, true, dpi)
                    {
                        run.k = k;
                        run.baked = Some(lane);
                    }
                }
                (false, Some(_)) => {}
            }
        }
        self.captured_runs = runs;
        self.cached_dynamic_batch = Some(batch);
    }
}
