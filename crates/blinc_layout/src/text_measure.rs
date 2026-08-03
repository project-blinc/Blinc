//! Text measurement for layout
//!
//! Provides a trait for measuring text dimensions during layout.
//! This allows accurate text sizing without estimation.

/// Text layout options that affect measurement
#[derive(Debug, Clone, Default)]
pub struct TextLayoutOptions {
    /// Line height multiplier (1.0 = default, 1.5 = 150%)
    pub line_height: f32,
    /// Extra spacing between letters in pixels
    pub letter_spacing: f32,
    /// Extra spacing between words in pixels
    pub word_spacing: f32,
    /// Maximum width for wrapping (None = no wrapping)
    pub max_width: Option<f32>,
    /// Font family name (e.g., "Fira Code", None for default)
    pub font_name: Option<String>,
    /// Generic font category
    pub generic_font: crate::div::GenericFont,
    /// Font weight (100-900, 400 = normal, 700 = bold)
    pub font_weight: u16,
    /// Whether text is italic
    pub italic: bool,
}

impl TextLayoutOptions {
    /// Create default options
    pub fn new() -> Self {
        Self {
            line_height: 1.2, // Default line height
            letter_spacing: 0.0,
            word_spacing: 0.0,
            max_width: None,
            font_name: None,
            generic_font: crate::div::GenericFont::System,
            font_weight: 400,
            italic: false,
        }
    }

    /// Set line height multiplier
    pub fn with_line_height(mut self, height: f32) -> Self {
        self.line_height = height;
        self
    }

    /// Set letter spacing
    pub fn with_letter_spacing(mut self, spacing: f32) -> Self {
        self.letter_spacing = spacing;
        self
    }

    /// Set word spacing
    pub fn with_word_spacing(mut self, spacing: f32) -> Self {
        self.word_spacing = spacing;
        self
    }

    /// Set max width for wrapping
    pub fn with_max_width(mut self, width: f32) -> Self {
        self.max_width = Some(width);
        self
    }

    /// Set font name
    pub fn with_font_name(mut self, name: impl Into<String>) -> Self {
        self.font_name = Some(name.into());
        self
    }

    /// Set generic font category
    pub fn with_generic_font(mut self, generic: crate::div::GenericFont) -> Self {
        self.generic_font = generic;
        self
    }

    /// Set monospace font
    pub fn monospace(mut self) -> Self {
        self.generic_font = crate::div::GenericFont::Monospace;
        self
    }

    /// Set font weight (100-900, 400 = normal, 700 = bold)
    pub fn with_weight(mut self, weight: u16) -> Self {
        self.font_weight = weight;
        self
    }

    /// Set bold weight (700)
    pub fn bold(mut self) -> Self {
        self.font_weight = 700;
        self
    }

    /// Set italic style
    pub fn with_italic(mut self, italic: bool) -> Self {
        self.italic = italic;
        self
    }

    /// Set italic style
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
}

/// Text measurement result
#[derive(Debug, Clone, Copy, Default)]
pub struct TextMetrics {
    /// Width in pixels
    pub width: f32,
    /// Height in pixels (accounts for line height and number of lines)
    pub height: f32,
    /// Ascender in pixels (distance from baseline to top)
    pub ascender: f32,
    /// Descender in pixels (distance from baseline to bottom, typically negative)
    pub descender: f32,
    /// Number of lines (1 for single-line text)
    pub line_count: u32,
}

/// One wrapped line of a measured string
///
/// Byte offsets index the string that was measured, so a caller holding spans
/// over that same string (link ranges, styled runs) can ask which line each one
/// landed on once a width is known.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineSpan {
    /// Byte offset where this line starts
    pub start: usize,
    /// Byte offset one past this line's last byte
    pub end: usize,
    /// Rendered width of the line in pixels
    pub width: f32,
}

/// Trait for measuring text dimensions
///
/// Implement this trait to provide accurate text measurement during layout.
/// Without a text measurer, text elements will use estimated sizes.
pub trait TextMeasurer: Send + Sync {
    /// Measure the dimensions of a text string with full layout options
    ///
    /// # Arguments
    /// * `text` - The text to measure
    /// * `font_size` - Font size in pixels
    /// * `options` - Layout options (line height, spacing, max width)
    ///
    /// # Returns
    /// `TextMetrics` with the measured dimensions
    fn measure_with_options(
        &self,
        text: &str,
        font_size: f32,
        options: &TextLayoutOptions,
    ) -> TextMetrics;

    /// Measure text with default options (convenience method)
    fn measure(&self, text: &str, font_size: f32) -> TextMetrics {
        self.measure_with_options(text, font_size, &TextLayoutOptions::new())
    }

    /// Where the wrap points fall for `text` under `options`
    ///
    /// The default reports the whole string as one line, which is what a
    /// measurer that cannot wrap should say. Implementations backed by a real
    /// layout engine override this so callers see the same line breaks the
    /// renderer will draw.
    fn line_spans(&self, text: &str, font_size: f32, options: &TextLayoutOptions) -> Vec<LineSpan> {
        let metrics = self.measure_with_options(text, font_size, options);
        vec![LineSpan {
            start: 0,
            end: text.len(),
            width: metrics.width,
        }]
    }
}

/// A dummy text measurer that uses estimates
///
/// This is used when no real text measurer is available.
/// Uses the same estimation formula as the fallback in text.rs.
#[derive(Debug, Clone, Copy, Default)]
pub struct EstimatedTextMeasurer;

impl EstimatedTextMeasurer {
    /// Width of a run under the estimation formula
    fn estimate_run_width(text: &str, font_size: f32, options: &TextLayoutOptions) -> f32 {
        let char_count = text.chars().count() as f32;
        let word_count = text.split_whitespace().count().max(1) as f32;
        let base = char_count * font_size * 0.55;
        let letters = (char_count - 1.0).max(0.0) * options.letter_spacing;
        let words = (word_count - 1.0).max(0.0) * options.word_spacing;
        base + letters + words
    }

    /// Greedy word wrap over the estimated widths
    ///
    /// Shares its answer with `measure_with_options` so the reported line
    /// count and the reported wrap points cannot disagree.
    fn wrap(text: &str, font_size: f32, options: &TextLayoutOptions) -> Vec<LineSpan> {
        let Some(max_width) = options.max_width.filter(|w| *w > 0.0) else {
            return vec![LineSpan {
                start: 0,
                end: text.len(),
                width: Self::estimate_run_width(text, font_size, options),
            }];
        };

        let mut lines: Vec<LineSpan> = Vec::new();
        let mut line_start = 0usize;
        let mut line_end = 0usize;

        for (offset, word) in text.split_whitespace().map(|w| {
            // `split_whitespace` drops positions, so recover each word's offset.
            let off = w.as_ptr() as usize - text.as_ptr() as usize;
            (off, w)
        }) {
            let candidate_end = offset + word.len();
            let candidate = &text[line_start..candidate_end];
            let width = Self::estimate_run_width(candidate, font_size, options);

            if width > max_width && line_end > line_start {
                lines.push(LineSpan {
                    start: line_start,
                    end: line_end,
                    width: Self::estimate_run_width(
                        &text[line_start..line_end],
                        font_size,
                        options,
                    ),
                });
                line_start = offset;
            }
            line_end = candidate_end;
        }

        lines.push(LineSpan {
            start: line_start,
            end: text.len().max(line_start),
            width: Self::estimate_run_width(&text[line_start..], font_size, options),
        });
        lines
    }
}

impl TextMeasurer for EstimatedTextMeasurer {
    fn measure_with_options(
        &self,
        text: &str,
        font_size: f32,
        options: &TextLayoutOptions,
    ) -> TextMetrics {
        let char_count = text.chars().count() as f32;
        let word_count = text.split_whitespace().count().max(1) as f32;

        // Base width: ~0.55 * font_size per character (conservative for proportional fonts)
        let base_char_width = font_size * 0.55;
        let base_width = char_count * base_char_width;

        // Add letter spacing (per character gap)
        let letter_spacing_total = if char_count > 1.0 {
            (char_count - 1.0) * options.letter_spacing
        } else {
            0.0
        };

        // Add word spacing (per word gap)
        let word_spacing_total = if word_count > 1.0 {
            (word_count - 1.0) * options.word_spacing
        } else {
            0.0
        };

        let total_width = base_width + letter_spacing_total + word_spacing_total;

        // Handle wrapping if max_width is set
        let (width, line_count) = if let Some(max_width) = options.max_width {
            if total_width > max_width && max_width > 0.0 {
                let lines = Self::wrap(text, font_size, options);
                let widest = lines.iter().map(|l| l.width).fold(0.0_f32, f32::max);
                (widest.min(max_width), lines.len().max(1) as u32)
            } else {
                (total_width, 1)
            }
        } else {
            (total_width, 1)
        };

        // Height based on line height and number of lines
        let line_height_px = font_size * options.line_height;
        let height = line_height_px * line_count as f32;

        // Ascender/descender estimates
        let ascender = font_size * 0.8;
        let descender = font_size * -0.2;

        TextMetrics {
            width,
            height,
            ascender,
            descender,
            line_count,
        }
    }

    fn line_spans(&self, text: &str, font_size: f32, options: &TextLayoutOptions) -> Vec<LineSpan> {
        Self::wrap(text, font_size, options)
    }
}

/// Global text measurer storage
///
/// This allows setting a text measurer that will be used during layout.
use std::sync::{Arc, RwLock};

static TEXT_MEASURER: RwLock<Option<Arc<dyn TextMeasurer>>> = RwLock::new(None);

/// Set the global text measurer
///
/// Call this at app initialization with a real text measurer
/// (e.g., one backed by the font rendering system).
pub fn set_text_measurer(measurer: Arc<dyn TextMeasurer>) {
    let mut guard = TEXT_MEASURER.write().unwrap();
    *guard = Some(measurer);
}

/// Clear the global text measurer
pub fn clear_text_measurer() {
    let mut guard = TEXT_MEASURER.write().unwrap();
    *guard = None;
}

/// Measure text using the global measurer, or fall back to estimation
pub fn measure_text(text: &str, font_size: f32) -> TextMetrics {
    let guard = TEXT_MEASURER.read().unwrap();
    if let Some(ref measurer) = *guard {
        measurer.measure(text, font_size)
    } else {
        EstimatedTextMeasurer.measure(text, font_size)
    }
}

/// Wrap points for `text` using the global measurer, or fall back to estimation
pub fn text_line_spans(text: &str, font_size: f32, options: &TextLayoutOptions) -> Vec<LineSpan> {
    let guard = TEXT_MEASURER.read().unwrap();
    if let Some(ref measurer) = *guard {
        measurer.line_spans(text, font_size, options)
    } else {
        EstimatedTextMeasurer.line_spans(text, font_size, options)
    }
}

/// Measure text with options using the global measurer, or fall back to estimation
pub fn measure_text_with_options(
    text: &str,
    font_size: f32,
    options: &TextLayoutOptions,
) -> TextMetrics {
    let guard = TEXT_MEASURER.read().unwrap();
    if let Some(ref measurer) = *guard {
        measurer.measure_with_options(text, font_size, options)
    } else {
        EstimatedTextMeasurer.measure_with_options(text, font_size, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "the quick brown fox jumps over the lazy dog";

    fn wrapped(max_width: f32) -> Vec<LineSpan> {
        let mut options = TextLayoutOptions::new();
        options.max_width = Some(max_width);
        EstimatedTextMeasurer.line_spans(TEXT, 16.0, &options)
    }

    #[test]
    fn unbounded_text_is_one_span_over_the_whole_string() {
        let spans = EstimatedTextMeasurer.line_spans(TEXT, 16.0, &TextLayoutOptions::new());
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].start, spans[0].end), (0, TEXT.len()));
    }

    #[test]
    fn a_narrower_container_yields_more_lines() {
        assert!(wrapped(200.0).len() > wrapped(600.0).len());
    }

    #[test]
    fn spans_cover_the_string_in_order_without_overlapping() {
        let spans = wrapped(200.0);
        assert_eq!(spans.first().unwrap().start, 0);
        assert_eq!(spans.last().unwrap().end, TEXT.len());
        for pair in spans.windows(2) {
            assert!(pair[0].end <= pair[1].start, "{pair:?} overlap");
        }
    }

    #[test]
    fn no_line_is_wider_than_the_container() {
        // Greedy wrap may only overflow on a single unbreakable word, and
        // this text has none.
        for span in wrapped(200.0) {
            assert!(span.width <= 200.0, "{span:?} exceeds the container");
        }
    }

    #[test]
    fn lines_break_on_word_boundaries() {
        for span in wrapped(200.0) {
            let line = &TEXT[span.start..span.end];
            assert_eq!(line.trim(), line, "{line:?} carries edge whitespace");
            assert!(TEXT[..span.start].is_empty() || TEXT[..span.start].ends_with(' '));
        }
    }

    #[test]
    fn the_reported_line_count_matches_the_reported_spans() {
        let mut options = TextLayoutOptions::new();
        options.max_width = Some(200.0);
        let metrics = EstimatedTextMeasurer.measure_with_options(TEXT, 16.0, &options);
        assert_eq!(metrics.line_count as usize, wrapped(200.0).len());
    }
}
