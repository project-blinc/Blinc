//! Text measurement using actual font metrics
//!
//! Provides accurate text measurement for layout by using the same font
//! as the renderer.

use blinc_layout::GenericFont as LayoutGenericFont;
use blinc_layout::text_measure::{LineSpan, TextLayoutOptions, TextMeasurer, TextMetrics};
use blinc_text::{FontFace, FontRegistry, GenericFont, LayoutOptions, TextLayoutEngine};
use std::sync::{Arc, Mutex};

/// Convert from layout's GenericFont to text's GenericFont
fn to_text_generic_font(layout_font: LayoutGenericFont) -> GenericFont {
    match layout_font {
        LayoutGenericFont::System => GenericFont::System,
        LayoutGenericFont::Monospace => GenericFont::Monospace,
        LayoutGenericFont::Serif => GenericFont::Serif,
        LayoutGenericFont::SansSerif => GenericFont::SansSerif,
    }
}

/// A text measurer that uses actual font metrics
///
/// This measurer uses the same font loading logic as the renderer
/// to provide accurate text dimensions for layout.
pub struct FontTextMeasurer {
    /// The font face to use for measurement (default/sans-serif)
    font: Arc<Mutex<Option<FontFace>>>,
    /// Font registry for loading different font families
    font_registry: Arc<Mutex<FontRegistry>>,
    /// The layout engine for measuring text
    layout_engine: Mutex<TextLayoutEngine>,
}

impl FontTextMeasurer {
    /// Create a new font text measurer.
    ///
    /// Uses the global shared font registry to minimize memory usage.
    /// Apple Color Emoji alone is 180MB - sharing prevents loading it multiple times.
    pub fn new() -> Self {
        let mut measurer = Self {
            font: Arc::new(Mutex::new(None)),
            font_registry: blinc_text::global_font_registry(),
            layout_engine: Mutex::new(TextLayoutEngine::new()),
        };
        measurer.load_system_font();
        measurer
    }

    /// Create a font text measurer with a shared font registry
    ///
    /// Use this to share the font registry with the text renderer,
    /// ensuring consistent font loading and metrics between measurement
    /// and rendering.
    pub fn with_shared_registry(font_registry: Arc<Mutex<FontRegistry>>) -> Self {
        // Note: system font loading is skipped since the registry is shared
        // and should already be initialized by the renderer
        Self {
            font: Arc::new(Mutex::new(None)),
            font_registry,
            layout_engine: Mutex::new(TextLayoutEngine::new()),
        }
    }

    /// Load the system default font
    fn load_system_font(&mut self) {
        for font_path in crate::system_font_paths() {
            let path = std::path::Path::new(font_path);
            if path.exists() {
                if let Ok(data) = std::fs::read(path) {
                    if let Ok(font) = FontFace::from_data(data) {
                        *self.font.lock().unwrap() = Some(font);
                        break;
                    }
                }
            }
        }
    }

    /// Load a custom font from data
    pub fn load_font_data(&self, data: Vec<u8>) -> Result<(), blinc_text::TextError> {
        let font = FontFace::from_data(data)?;
        *self.font.lock().unwrap() = Some(font);
        Ok(())
    }

    /// Fallback estimation when no font is loaded
    fn estimate_size(text: &str, font_size: f32, options: &TextLayoutOptions) -> TextMetrics {
        let char_count = text.chars().count() as f32;
        let word_count = text.split_whitespace().count().max(1) as f32;

        // Base width: ~0.55 * font_size per character
        let base_char_width = font_size * 0.55;
        let base_width = char_count * base_char_width;

        // Add letter spacing
        let letter_spacing_total = if char_count > 1.0 {
            (char_count - 1.0) * options.letter_spacing
        } else {
            0.0
        };

        // Add word spacing
        let word_spacing_total = if word_count > 1.0 {
            (word_count - 1.0) * options.word_spacing
        } else {
            0.0
        };

        let total_width = base_width + letter_spacing_total + word_spacing_total;

        // Handle wrapping
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
        let one_line = || {
            vec![LineSpan {
                start: 0,
                end: text.len(),
                width: self.measure_with_options(text, font_size, options).width,
            }]
        };

        // Only a definite width produces wrap points. MinContent and MaxContent
        // (see `measure_with_options`) are sizing probes, not a layout.
        let Some(max_width) = options.max_width.filter(|w| *w > 0.0) else {
            return one_line();
        };

        let registry = self.font_registry.lock().unwrap();
        let font = match registry.get_for_render_with_style(
            options.font_name.as_deref(),
            to_text_generic_font(options.generic_font),
            options.font_weight,
            options.italic,
        ) {
            Some(f) => f,
            None => return one_line(),
        };
        drop(registry);

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

/// Initialize the global text measurer with font support
///
/// Call this at application startup to enable accurate text measurement.
/// This should be called before any UI elements are created.
///
/// Note: For optimal text rendering, use `init_text_measurer_with_registry`
/// to share the font registry with the text renderer.
pub fn init_text_measurer() {
    let measurer = Arc::new(FontTextMeasurer::new());
    blinc_layout::set_text_measurer(measurer);
}

/// Initialize the global text measurer with a shared font registry
///
/// This ensures the text measurer uses the same fonts as the renderer,
/// providing accurate text measurement that matches rendered text exactly.
///
/// Call this after creating the BlincApp/TextRenderingContext:
///
/// ```ignore
/// let (app, surface) = BlincApp::with_window(window, None)?;
/// init_text_measurer_with_registry(app.font_registry());
/// ```
pub fn init_text_measurer_with_registry(font_registry: Arc<Mutex<FontRegistry>>) {
    let measurer = Arc::new(FontTextMeasurer::with_shared_registry(font_registry));
    blinc_layout::set_text_measurer(measurer);
}
