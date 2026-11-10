//! Text drawing through the text rendering context.

use super::GpuPaintContext;
use crate::primitives::GpuPrimitive;
use blinc_core::{Point, TextStyle};

impl GpuPaintContext<'_> {
    pub(super) fn draw_text_impl(&mut self, text: &str, origin: Point, style: &TextStyle) {
        use blinc_core::{TextAlign, TextBaseline};
        use blinc_text::{TextAlignment, TextAnchor};

        // Check if text context is available
        if self.text_ctx.is_none() {
            return;
        }

        // Transform origin by current transform (includes DPI scale
        // + any CSS / scroll translation from ancestors).
        let transformed_origin = self.transform_point(origin);

        // Extract uniform scale (DPI + container scale) so the glyph
        // rasterisation matches the post-transform size on screen.
        // Without this the text_ctx rasterises at CSS-pixel size
        // while the vertex shader places the quad at physical-pixel
        // coordinates — text renders at roughly half-size on 2×
        // retina and is horizontally offset because the glyph
        // advances don't keep up with the transformed origin.
        let uniform_scale = self.current_uniform_scale();
        let scaled_size = style.size * uniform_scale;

        // Glyph rasterisation FLOOR + CEILING. Without these the
        // raster size tracks the post-transform display size 1:1,
        // and the atlas size-quantiser (`(font_size*2.0).round()`)
        // creates a fresh bucket per ~0.5px display-size band.
        // Zoom-out below ~4px hits empty hinted bitmaps and the
        // text disappears; zoom-in pumps a new atlas entry for
        // every zoom step, releasing + re-rasterising every glyph
        // on the screen each frame the zoom moves — that's the
        // "lag worsens as I zoom in deeper, persists when I zoom
        // back out" symptom.
        //
        // Cap raster at [FLOOR..CEILING], scale the quad to make
        // up the remainder. At display sizes between FLOOR and
        // CEILING the raster matches the display exactly
        // (`quad_scale == 1.0`, original pixel-perfect path).
        // Beyond CEILING the same atlas glyph is reused regardless
        // of zoom; the magnified quad samples it with Linear
        // mag_filter for smoothness.
        const TEXT_RASTER_FLOOR: f32 = 6.0;
        // Ceiling lowered from 48 → 32. The atlas allocates one
        // shelf per (font_id, glyph_id, size) tuple; each shelf
        // takes raster_size² pixels (+padding). A canvas zooming
        // 1x → max accumulates buckets at raster=12, 24, 36
        // (display scaled_size before clamp). Higher ceiling
        // means each bucket is bigger AND more sizes used. At 32
        // the shelf is 56% smaller than at 48 and only sizes
        // 6, 12, 18, 24, 32 land before the clamp engages —
        // enough resolution for legible text up to ~150% the
        // ceiling display size without the atlas filling on
        // intermediate zoom levels and starving HUD text inserts.
        const TEXT_RASTER_CEILING: f32 = 32.0;
        let raster_size = scaled_size.clamp(TEXT_RASTER_FLOOR, TEXT_RASTER_CEILING);
        let quad_scale = if raster_size > 0.0 {
            scaled_size / raster_size
        } else {
            1.0
        };

        // Get current opacity
        let opacity = self.combined_opacity();

        // Resolve the current rect clip (if any) so glyphs outside
        // the active scissor get clipped. `get_clip_data` returns a
        // large default bound when the stack is empty; when a rect
        // clip is active the third element carries its `ClipType`,
        // and we propagate that onto each glyph so the shader's
        // `PRIM_TEXT` branch gates `clip_edge_alpha` properly.
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();
        let clip_kind_u32 = clip_type as u32;

        // Convert TextStyle color to [f32; 4] with opacity applied
        let color = [
            style.color.r,
            style.color.g,
            style.color.b,
            style.color.a * opacity,
        ];

        // Map TextAlign to TextAlignment
        let alignment = match style.align {
            TextAlign::Left => TextAlignment::Left,
            TextAlign::Center => TextAlignment::Center,
            TextAlign::Right => TextAlignment::Right,
        };

        // Map TextBaseline to TextAnchor
        let anchor = match style.baseline {
            TextBaseline::Top => TextAnchor::Top,
            TextBaseline::Middle => TextAnchor::Center,
            TextBaseline::Alphabetic => TextAnchor::Baseline,
            TextBaseline::Bottom => TextAnchor::Baseline, // Approximate with baseline
        };

        // Map `TextStyle::family` onto the font name that
        // `prepare_text_with_style` passes into
        // `blinc_text::Renderer::resolve_font_with_style`. The
        // previous `prepare_text_with_options` route always passed
        // `font_name=None`, so any caller-set family on `TextStyle`
        // (e.g. a Lottie text layer asking for "Cal Sans") silently
        // dropped through to the default SansSerif fallback. Treat
        // the blinc_core default "system-ui" as "no preference" so
        // Blinc's own generic-font routing still kicks in.
        let family_opt = if style.family == "system-ui" || style.family.is_empty() {
            None
        } else {
            Some(style.family.as_str())
        };
        let (weight_u16, italic_flag) = match style.weight {
            blinc_core::FontWeight::Thin => (100u16, false),
            blinc_core::FontWeight::Light => (300u16, false),
            blinc_core::FontWeight::Regular => (400u16, false),
            blinc_core::FontWeight::Medium => (500u16, false),
            blinc_core::FontWeight::Bold => (700u16, false),
            blinc_core::FontWeight::Black => (900u16, false),
        };

        // Now borrow text_ctx and prepare glyphs
        let text_ctx = self.text_ctx.as_mut().unwrap();
        if let Ok(mut glyphs) = text_ctx.prepare_text_with_style(
            text,
            transformed_origin.x,
            transformed_origin.y,
            raster_size,
            color,
            anchor,
            alignment,
            None,  // No width constraint
            false, // No wrap for canvas text
            family_opt,
            blinc_text::GenericFont::SansSerif,
            weight_u16,
            italic_flag,
            None,
            style.letter_spacing,
        ) {
            // When raster size was clamped (floor for zoom-out OR
            // ceiling for zoom-in), the layout above ran at
            // `raster_size`. Scale each glyph quad's offset from
            // the text origin AND its w/h by `quad_scale` so the
            // prim covers the intended display footprint while
            // sampling the clamped atlas region. UV bounds stay
            // unchanged. `quad_scale != 1.0` covers BOTH the
            // minify direction (deep zoom-out → Linear min_filter
            // smooth fade) and the magnify direction (deep zoom-in
            // → Linear mag_filter smooth upscale).
            if (quad_scale - 1.0).abs() > 1e-4 {
                for glyph in &mut glyphs {
                    let dx = glyph.bounds[0] - transformed_origin.x;
                    let dy = glyph.bounds[1] - transformed_origin.y;
                    glyph.bounds[0] = transformed_origin.x + dx * quad_scale;
                    glyph.bounds[1] = transformed_origin.y + dy * quad_scale;
                    glyph.bounds[2] *= quad_scale;
                    glyph.bounds[3] *= quad_scale;
                }
            }
            // Apply current clip bounds and fade to all glyphs
            let glyph_clip_fade = self.get_clip_fade();
            for glyph in &mut glyphs {
                glyph.clip_bounds = clip_bounds;
                glyph.clip_fade = glyph_clip_fade;
            }

            // Route each glyph straight into the primitive stream
            // instead of the `batch.glyphs` vec. That vec is only
            // drained by `convert_glyphs_to_primitives` /
            // `get_unified_foreground_primitives` on the *foreground*
            // batch — canvas elements render in the background layer
            // by default, so pushing into `glyphs` would silently
            // drop their text. `GpuPrimitive::from_glyph` carries
            // the glyph's texture coords + colour so the SDF
            // pipeline that consumes `primitives` renders identical
            // output. We also stamp the active clip onto each prim
            // so the shader's PRIM_TEXT branch respects scroll /
            // overflow-clip boundaries (canvas text used to spill
            // past its element because the default was
            // `ClipType::None` regardless of stack state).
            for glyph in glyphs {
                let mut prim = GpuPrimitive::from_glyph(&glyph);
                prim.type_info[2] = clip_kind_u32;
                prim.clip_bounds = clip_bounds;
                prim.clip_radius = clip_radius;
                self.emit_native_primitive(prim);
            }
        }
    }
}
