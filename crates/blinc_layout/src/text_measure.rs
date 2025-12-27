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
}

/// A dummy text measurer that uses estimates
///
/// This is used when no real text measurer is available.
/// Uses the same estimation formula as the fallback in text.rs.
#[derive(Debug, Clone, Copy, Default)]
pub struct EstimatedTextMeasurer;

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
                // Estimate number of lines needed
                let lines = (total_width / max_width).ceil() as u32;
                (max_width, lines.max(1))
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
