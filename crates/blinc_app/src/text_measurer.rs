//! Text measurement using actual font metrics
//!
//! Provides accurate text measurement for layout by using the same font
//! as the renderer.

use blinc_layout::text_measure::{TextLayoutOptions, TextMeasurer, TextMetrics};
use blinc_layout::GenericFont as LayoutGenericFont;
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
    /// Create a new font text measurer
    pub fn new() -> Self {
        let mut measurer = Self {
            font: Arc::new(Mutex::new(None)),
            font_registry: Arc::new(Mutex::new(FontRegistry::new())),
            layout_engine: Mutex::new(TextLayoutEngine::new()),
        };
        measurer.load_system_font();
        measurer
    }

    /// Load the system default font
    fn load_system_font(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let font_path = std::path::Path::new("/System/Library/Fonts/Helvetica.ttc");
            if font_path.exists() {
                if let Ok(data) = std::fs::read(font_path) {
                    if let Ok(font) = FontFace::from_data(data) {
                        *self.font.lock().unwrap() = Some(font);
                    }
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            let font_paths = [
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
            ];
            for path in &font_paths {
                if let Ok(data) = std::fs::read(path) {
                    if let Ok(font) = FontFace::from_data(data) {
                        *self.font.lock().unwrap() = Some(font);
                        break;
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            let font_path = "C:\\Windows\\Fonts\\segoeui.ttf";
            if let Ok(data) = std::fs::read(font_path) {
                if let Ok(font) = FontFace::from_data(data) {
                    *self.font.lock().unwrap() = Some(font);
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
        let registry = self.font_registry.lock().unwrap();
        let font = match registry.get_for_render(
            options.font_name.as_deref(),
            generic_font,
        ) {
            Some(f) => f,
            None => return Self::estimate_size(text, font_size, options),
        };
        drop(registry); // Release lock before layout

        // Convert our options to blinc_text options
        let mut layout_opts = LayoutOptions::default();
        layout_opts.line_height = options.line_height;
        layout_opts.letter_spacing = options.letter_spacing;
        if let Some(max_width) = options.max_width {
            layout_opts.max_width = Some(max_width);
        } else {
            // No wrapping for single-line measurement
            layout_opts.line_break = blinc_text::LineBreakMode::None;
        }

        let layout_engine = self.layout_engine.lock().unwrap();
        let layout = layout_engine.layout(text, &font, font_size, &layout_opts);

        // Get font metrics
        let metrics = font.metrics();
        let ascender = metrics.ascender_px(font_size);
        let descender = metrics.descender_px(font_size);

        TextMetrics {
            width: layout.width,
            height: layout.height,
            ascender,
            descender,
            line_count: layout.lines.len() as u32,
        }
    }
}

/// Initialize the global text measurer with font support
///
/// Call this at application startup to enable accurate text measurement.
/// This should be called before any UI elements are created.
pub fn init_text_measurer() {
    let measurer = Arc::new(FontTextMeasurer::new());
    blinc_layout::set_text_measurer(measurer);
}
