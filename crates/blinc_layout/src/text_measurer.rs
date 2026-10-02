//! Font-backed text measurement.
//!
//! Measures with the same fonts and shaper the renderer draws with, so
//! layout agrees with what lands on screen. Without this installed a
//! `LayoutTree` falls back to [`crate::text_measure::EstimatedTextMeasurer`],
//! which guesses a
//! width per character and gets narrow and wide glyphs equally wrong.
//!
//! Needs no renderer and no windowing: a host driving a `LayoutTree`
//! directly can call [`init_text_measurer`] and measure properly.

use std::sync::{Arc, Mutex};

use blinc_text::{FontRegistry, GenericFont, LayoutOptions, TextLayoutEngine};

use crate::GenericFont as LayoutGenericFont;
use crate::text_measure::{
    EstimatedTextMeasurer, LineSpan, TextLayoutOptions, TextMeasurer, TextMetrics,
};

/// Convert from layout's `GenericFont` to text's.
fn to_text_generic_font(layout_font: LayoutGenericFont) -> GenericFont {
    match layout_font {
        LayoutGenericFont::System => GenericFont::System,
        LayoutGenericFont::Monospace => GenericFont::Monospace,
        LayoutGenericFont::Serif => GenericFont::Serif,
        LayoutGenericFont::SansSerif => GenericFont::SansSerif,
    }
}

/// A text measurer backed by real font metrics.
pub struct FontTextMeasurer {
    /// Where faces are looked up. Shared with the renderer when there is
    /// one, so measurement and drawing cannot disagree.
    font_registry: Arc<Mutex<FontRegistry>>,
    /// The shaper. Behind its own lock because shaping is not reentrant.
    layout_engine: Mutex<TextLayoutEngine>,
}

impl FontTextMeasurer {
    /// Build a measurer over the global shared registry, preloading the
    /// generic families so the first measurement has fonts to find.
    ///
    /// The preload is what makes this usable with no renderer:
    /// `get_for_render_with_style` only reads the cache, so an unpopulated
    /// registry silently measures every string as an estimate.
    pub fn new() -> Self {
        let font_registry = blinc_text::global_font_registry();
        if let Ok(mut registry) = font_registry.lock() {
            registry.preload_generic_fonts();
        }
        Self {
            font_registry,
            layout_engine: Mutex::new(TextLayoutEngine::new()),
        }
    }

    /// Build a measurer over a registry someone else owns.
    ///
    /// No preload: the caller sharing a registry is the renderer, which
    /// fills the cache as it draws, and preloading here would risk a
    /// system font scan during startup.
    pub fn with_shared_registry(font_registry: Arc<Mutex<FontRegistry>>) -> Self {
        Self {
            font_registry,
            layout_engine: Mutex::new(TextLayoutEngine::new()),
        }
    }

    /// Fallback when the registry has no face for what was asked.
    fn estimate_size(text: &str, font_size: f32, options: &TextLayoutOptions) -> TextMetrics {
        let char_count = text.chars().count() as f32;
        let word_count = text.split_whitespace().count().max(1) as f32;

        // Base width: ~0.55 * font_size per character
        let base_char_width = font_size * 0.55;
        let base_width = char_count * base_char_width;

        let letter_spacing_total = if char_count > 1.0 {
            (char_count - 1.0) * options.letter_spacing
        } else {
            0.0
        };

        let word_spacing_total = if word_count > 1.0 {
            (word_count - 1.0) * options.word_spacing
        } else {
            0.0
        };

        let total_width = base_width + letter_spacing_total + word_spacing_total;

        let (width, line_count) = if let Some(max_width) = options.max_width {
            if total_width > max_width && max_width > 0.0 {
                let lines = (total_width / max_width).ceil() as u32;
                (max_width, lines.max(1))
            } else {
                (total_width, 1)
            }
        } else {
            (total_width, 1)
        };

        let line_height_px = font_size * options.line_height;
        let height = line_height_px * line_count as f32;

        TextMetrics {
            width,
            height,
            ascender: font_size * 0.8,
            descender: font_size * -0.2,
            line_count,
        }
    }
}

impl Default for FontTextMeasurer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextMeasurer for FontTextMeasurer {
    fn measure_with_options(
        &self,
        text: &str,
        font_size: f32,
        options: &TextLayoutOptions,
    ) -> TextMetrics {
        // Determine which font to use based on options
        let generic_font = to_text_generic_font(options.generic_font);

        // Fast path: use cached fonts only (never load during measurement)
        // Use weight and italic from options to get the correct font variant
        let registry = self.font_registry.lock().unwrap();
        let font = match registry.get_for_render_with_style(
            options.font_name.as_deref(),
            generic_font,
            options.font_weight,
            options.italic,
        ) {
            Some(f) => f,
            None => return Self::estimate_size(text, font_size, options),
        };
        drop(registry); // Release lock before layout

        // `blinc_layout::tree::text_measure_function` encodes taffy's
        // three AvailableSpace variants as:
        //   Definite(w)  → `max_width = Some(w)` with w > 0
        //   MinContent   → `max_width = Some(0.0)`
        //   MaxContent   → `max_width = None`
        //
        // Each path needs distinct handling so taffy's flex sizing
        // doesn't end up inflating `h_fit` parents.
        //
        // - Definite: normal layout at `w`, wrap on word boundaries.
        //   Reports the actual multi-line height for a known width —
        //   what the visible layout will use.
        //
        // - MinContent: return the height of a SINGLE rendered line
        //   at no-wrap width, PLUS the width of the longest
        //   unbreakable run (longest word). The legacy behaviour laid
        //   out the whole text at `max_width = longest_word` which,
        //   for multi-word text, returned a height of `word_count ×
        //   line_height`. Two cards with differently-worded titles
        //   (e.g. "Coffee (.lottie)" → 2 words, "Sandy Loading
        //   (JSON)" → 3 words) then reported min-content heights
        //   proportional to their word counts, which taffy fed into
        //   the cross-axis sizing and produced visibly unequal card
        //   heights even though the definite-width layout would have
        //   given each a single line.
        //
        //   CSS's min-content height technically *is* the height at
        //   min-content width (potentially many lines), but taffy's
        //   flex algorithm uses this hint to bound the container,
        //   not to commit to a rendered height. Returning a single
        //   line here matches the height the actual layout pass
        //   will use whenever the container can fit the text on one
        //   row at its definite width — i.e. the common case for
        //   single-line titles — without affecting real multi-line
        //   content (it gets wrapped at the Definite pass). The
        //   alternative (CSS-correct multi-line min-content height)
        //   propagates through taffy as "this text needs N lines"
        //   and pushes every `h_fit` ancestor wider, which is the
        //   exact bug this block exists to prevent.
        //
        // - MaxContent: no wrap, single line height. Unchanged.
        let layout_engine = self.layout_engine.lock().unwrap();

        let probe = LayoutOptions {
            line_height: options.line_height,
            letter_spacing: options.letter_spacing,
            max_width: None,
            line_break: blinc_text::LineBreakMode::None,
            ..LayoutOptions::default()
        };
        let single_line = layout_engine.layout(text, &font, font_size, &probe);

        let (width, height, line_count) = match options.max_width {
            Some(mw) if mw > 0.0 => {
                let layout_opts = LayoutOptions {
                    line_height: options.line_height,
                    letter_spacing: options.letter_spacing,
                    max_width: Some(mw),
                    line_break: blinc_text::LineBreakMode::Word,
                    ..LayoutOptions::default()
                };
                let laid = layout_engine.layout(text, &font, font_size, &layout_opts);
                (laid.width, laid.height, laid.lines.len() as u32)
            }
            Some(_) => {
                // MinContent: width = longest word, height = one line.
                let longest_word = text
                    .split_whitespace()
                    .map(|w| layout_engine.layout(w, &font, font_size, &probe).width)
                    .fold(0.0_f32, f32::max);
                let mc_width = longest_word.max(1.0).min(single_line.width.max(1.0));
                (mc_width, single_line.height, 1)
            }
            None => {
                // MaxContent: no-wrap single line.
                (single_line.width, single_line.height, 1)
            }
        };

        let metrics = font.metrics();
        let ascender = metrics.ascender_px(font_size);
        let descender = metrics.descender_px(font_size);

        TextMetrics {
            width,
            height,
            ascender,
            descender,
            line_count,
        }
    }

    fn line_spans(&self, text: &str, font_size: f32, options: &TextLayoutOptions) -> Vec<LineSpan> {
        // Whatever wrapped the layout must wrap the hit geometry, so this
        // falls back exactly as `measure_with_options` does. Reporting a
        // single line here instead left link rects unwrapped while taffy
        // had already broken the text across lines.
        let fallback = || EstimatedTextMeasurer.line_spans(text, font_size, options);

        // Only a definite width produces wrap points. MinContent and MaxContent
        // (see `measure_with_options`) are sizing probes, not a layout.
        let Some(max_width) = options.max_width.filter(|w| *w > 0.0) else {
            return fallback();
        };

        // Scoped so the guard is gone before the fallback, which measures
        // and so takes this same non-reentrant lock. Returning out of the
        // guard's scope deadlocked instead of falling back.
        let font = {
            let registry = self.font_registry.lock().unwrap();
            registry.get_for_render_with_style(
                options.font_name.as_deref(),
                to_text_generic_font(options.generic_font),
                options.font_weight,
                options.italic,
            )
        };
        let Some(font) = font else {
            return fallback();
        };

        let layout_opts = LayoutOptions {
            line_height: options.line_height,
            letter_spacing: options.letter_spacing,
            max_width: Some(max_width),
            line_break: blinc_text::LineBreakMode::Word,
            ..LayoutOptions::default()
        };
        let laid = self
            .layout_engine
            .lock()
            .unwrap()
            .layout(text, &font, font_size, &layout_opts);

        laid.line_byte_ranges(text.len())
            .into_iter()
            .zip(laid.lines.iter())
            .map(|(range, line)| LineSpan {
                start: range.start,
                end: range.end,
                width: line.width,
            })
            .collect()
    }
}

/// Install a font-backed measurer as the global one.
///
/// Call it before building any tree that holds text. It preloads the
/// generic font families, so it works with no renderer and no
/// `blinc_app` — which is the point of it living here.
///
/// With a renderer, prefer [`init_text_measurer_with_registry`] so
/// measurement and drawing share one registry.
pub fn init_text_measurer() {
    crate::set_text_measurer(Arc::new(FontTextMeasurer::new()));
}

/// Install a font-backed measurer over a registry the renderer owns.
///
/// Measurement then resolves the same faces the renderer draws with, so
/// a measured width matches the drawn one exactly.
///
/// ```ignore
/// let (app, surface) = BlincApp::with_window(window, None)?;
/// init_text_measurer_with_registry(app.font_registry());
/// ```
pub fn init_text_measurer_with_registry(font_registry: Arc<Mutex<FontRegistry>>) {
    crate::set_text_measurer(Arc::new(FontTextMeasurer::with_shared_registry(
        font_registry,
    )));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_measure::measure_text_with_options;

    /// The whole point of this module: a tree with no renderer and no
    /// `blinc_app` measures with real glyph advances.
    ///
    /// `EstimatedTextMeasurer` bills every character at `0.55 * font_size`,
    /// so six narrow glyphs and six wide ones come out identical. A font
    /// disagrees, and by a wide margin for these two.
    #[test]
    fn narrow_and_wide_glyphs_measure_differently() {
        init_text_measurer();

        let opts = TextLayoutOptions::new();
        let narrow = measure_text_with_options("iiiiii", 32.0, &opts);
        let wide = measure_text_with_options("WWWWWW", 32.0, &opts);

        // A machine with none of the known font paths cannot prove
        // anything; skip rather than fail the suite on a bare container.
        let estimate = 6.0 * 32.0 * 0.55;
        if (narrow.width - estimate).abs() < 0.01 && (wide.width - estimate).abs() < 0.01 {
            eprintln!("no system font resolved; skipping");
            return;
        }

        assert!(
            wide.width > narrow.width * 1.5,
            "expected W to far outmeasure i, got narrow={} wide={}",
            narrow.width,
            wide.width
        );
        assert!(narrow.ascender > 0.0 && narrow.descender < 0.0);
    }

    /// The estimator is what you get without this installed, and it is
    /// blind to glyph width. Guards the contrast the test above relies on.
    #[test]
    fn the_estimator_cannot_tell_them_apart() {
        let opts = TextLayoutOptions::new();
        let narrow = EstimatedTextMeasurer.measure_with_options("iiiiii", 32.0, &opts);
        let wide = EstimatedTextMeasurer.measure_with_options("WWWWWW", 32.0, &opts);
        assert_eq!(narrow.width, wide.width);
    }
}
