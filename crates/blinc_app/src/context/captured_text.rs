//! Prepare collected text for a native group's ordered paint position.
use super::{
    GpuPrimitive, PrimitiveBatch, RenderContext, TextAlign, TextAlignment, TextAnchor, TextElement,
    TextVerticalAlign, cached_text, generate_text_decoration_primitives_by_layer, text_clip,
    to_gpu_generic_font,
};

impl RenderContext {
    pub(super) fn prepare_captured_text(
        &mut self,
        text: &TextElement,
        viewport: (u32, u32),
        dpi: f32,
    ) -> PrimitiveBatch {
        self.prepare_captured_text_at(text, viewport, dpi, 1.0, None)
    }

    /// [`prepare_captured_text`](Self::prepare_captured_text), laid out at the base size and
    /// rasterized `k` times finer with the quads shrunk back. `snap` of `Some` puts the glyphs
    /// of text that only moves and scales on the display grid, or leaves them smooth, as motion
    /// text is; `None` keeps the rule of text that is not moved by the node table, which rounds
    /// the glyph origins of text with no transform.
    pub(super) fn prepare_captured_text_at(
        &mut self,
        text: &TextElement,
        viewport: (u32, u32),
        dpi: f32,
        k: f32,
        snap: Option<bool>,
    ) -> PrimitiveBatch {
        let alignment = match text.align {
            TextAlign::Left => TextAlignment::Left,
            TextAlign::Center => TextAlignment::Center,
            TextAlign::Right => TextAlignment::Right,
        };
        let (anchor, y, height) = match text.v_align {
            TextVerticalAlign::Center => (TextAnchor::Center, text.y + text.height * 0.5, None),
            TextVerticalAlign::Top => (TextAnchor::Top, text.y, Some(text.height)),
            TextVerticalAlign::Baseline => (
                TextAnchor::Baseline,
                text.y + text.half_leading + text.ascender,
                None,
            ),
        };
        let effective_width = text
            .clip_bounds
            .map_or(text.width, |clip| clip[2].min(text.width));
        let wrap = text.wrap
            && effective_width < text.measured_width - super::cached_text::wrap_slack(dpi);
        let mut result = PrimitiveBatch::new();
        let mut paints = Vec::with_capacity(2);
        if let Some(shadow) = text.text_shadow {
            paints.push((
                text.x + shadow.offset_x * dpi,
                y + shadow.offset_y * dpi,
                [
                    shadow.color.r,
                    shadow.color.g,
                    shadow.color.b,
                    shadow.color.a,
                ],
            ));
        }
        paints.push((text.x, y, text.color));
        for (x, y, color) in paints {
            let mut glyphs = match self.text_ctx.prepare_text_with_style(
                &text.content,
                x * k,
                y * k,
                text.font_size * k,
                color,
                anchor,
                alignment,
                Some(text.width * k),
                wrap,
                text.font_family.name.as_deref(),
                to_gpu_generic_font(text.font_family.generic),
                text.weight.weight(),
                text.italic,
                height.map(|h| h * k),
                text.letter_spacing * k,
                text.word_spacing * k,
            ) {
                Ok(glyphs) => glyphs,
                Err(error) => {
                    tracing::warn!(?error, "Failed to prepare captured text");
                    continue;
                }
            };
            if k != 1.0 {
                for glyph in &mut glyphs {
                    for b in &mut glyph.bounds {
                        *b /= k;
                    }
                }
            }
            let mut owned = text_clip::prepare_glyphs(text, glyphs, viewport);
            if snap.is_none() && text.css_affine.is_none() {
                for glyph in &mut owned.glyphs {
                    glyph.bounds[0] = glyph.bounds[0].round();
                    glyph.bounds[1] = glyph.bounds[1].round();
                }
            }
            cached_text::convert_affine(
                &mut owned,
                text.css_affine.unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
                dpi,
                text.z_index,
            );
            if snap == Some(true) && super::frame_text::is_axis_aligned(text.css_affine) {
                super::frame_text::snap_to_pixels(&mut owned);
            }
            result.merge(owned);
        }

        let decorations = generate_text_decoration_primitives_by_layer(std::slice::from_ref(text));
        for primitives in decorations.values() {
            let mut owned = if let Some(path) = &text.path_clip {
                path.prepare_rects(primitives, text.clip_bounds, viewport)
            } else {
                let mut batch = PrimitiveBatch::new();
                batch.primitives = primitives.clone();
                batch
            };
            if let Some(affine) = text.css_affine {
                for primitive in &mut owned.primitives {
                    transform_rect(primitive, affine, dpi);
                }
            }
            result.merge(owned);
        }
        result
    }
}

fn transform_rect(primitive: &mut GpuPrimitive, affine: [f32; 6], dpi: f32) {
    let [a, b, c, d, tx, ty] = affine;
    let [x, y, w, h] = primitive.bounds;
    let (cx, cy) = (x + w * 0.5, y + h * 0.5);
    primitive.bounds[0] = a * cx + c * cy + tx * dpi - w * 0.5;
    primitive.bounds[1] = b * cx + d * cy + ty * dpi - h * 0.5;
    primitive.local_affine = [a, b, c, d];
}
