//! High-quality text rendering for Blinc UI framework
//!
//! This crate provides:
//! - Font loading and parsing (TTF/OTF via ttf-parser)
//! - Text shaping (HarfBuzz via rustybuzz)
//! - Glyph rasterization
//! - Glyph atlas management
//! - Text layout engine (line breaking, alignment)
//!
//! # Shared Font Registry
//!
//! To minimize memory usage (Apple Color Emoji is 180MB!), use the global shared
//! font registry instead of creating new FontRegistry instances:
//!
//! ```ignore
//! use blinc_text::global_font_registry;
//!
//! // All components share this single registry
//! let registry = global_font_registry();
//! ```

pub mod atlas;
pub mod emoji;
pub mod font;
pub mod layout;
pub mod rasterizer;
pub mod registry;
pub mod renderer;
pub mod shaper;

use std::sync::{Arc, Mutex, OnceLock};

pub use atlas::{AtlasRegion, ColorGlyphAtlas, GlyphAtlas, GlyphInfo};
pub use emoji::{contains_emoji, is_emoji, EmojiRenderer, EmojiSprite};
pub use font::{Font, FontFace, FontMetrics, FontStyle, FontWeight};

/// Global shared font registry singleton.
///
/// This ensures fonts (especially Apple Color Emoji at 180MB) are loaded once
/// and shared across all text rendering components.
static GLOBAL_FONT_REGISTRY: OnceLock<Arc<Mutex<registry::FontRegistry>>> = OnceLock::new();

/// Get the global shared font registry.
///
/// Returns a reference to the singleton font registry shared across all text
/// rendering components. Using this instead of creating new FontRegistry instances
/// saves significant memory (180MB+ for emoji font alone).
///
/// # Example
///
/// ```ignore
/// use blinc_text::{global_font_registry, TextRenderer, EmojiRenderer};
///
/// // All share the same fonts
/// let renderer = TextRenderer::with_shared_registry(global_font_registry());
/// let emoji = EmojiRenderer::with_registry(global_font_registry());
/// ```
pub fn global_font_registry() -> Arc<Mutex<registry::FontRegistry>> {
    Arc::clone(
        GLOBAL_FONT_REGISTRY.get_or_init(|| Arc::new(Mutex::new(registry::FontRegistry::new()))),
    )
}

// Re-export html-escape for entity decoding
pub use html_escape::decode_html_entities;
pub use layout::{
    LayoutOptions, LineBreakMode, PositionedGlyph, TextAlignment, TextAnchor, TextLayout,
    TextLayoutEngine,
};
pub use rasterizer::{GlyphFormat, GlyphRasterizer, RasterizedGlyph};
pub use registry::{FontRegistry, GenericFont};
pub use renderer::{ColorSpan, GlyphInstance, PreparedText, TextRenderer};
pub use shaper::{ShapedGlyph, ShapedText, TextShaper};

use thiserror::Error;

/// Text rendering errors
#[derive(Error, Debug)]
pub enum TextError {
    #[error("Failed to load font: {0}")]
    FontLoadError(String),

    #[error("Failed to parse font: {0}")]
    FontParseError(String),

    #[error("Glyph not found for codepoint: {0}")]
    GlyphNotFound(char),

    #[error("Atlas is full, cannot allocate glyph")]
    AtlasFull,

    #[error("Invalid font data")]
    InvalidFontData,
}

pub type Result<T> = std::result::Result<T, TextError>;
