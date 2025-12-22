//! High-quality text rendering for Blinc UI framework
//!
//! This crate provides:
//! - Font loading and parsing (TTF/OTF via ttf-parser)
//! - Text shaping (HarfBuzz via rustybuzz)
//! - Glyph rasterization
//! - Glyph atlas management
//! - Text layout engine (line breaking, alignment)

pub mod font;
pub mod shaper;
pub mod rasterizer;
pub mod atlas;
pub mod layout;
pub mod renderer;

pub use font::{Font, FontFace, FontWeight, FontStyle, FontMetrics};
pub use shaper::{ShapedGlyph, ShapedText, TextShaper};
pub use rasterizer::{GlyphRasterizer, RasterizedGlyph};
pub use atlas::{GlyphAtlas, GlyphInfo, AtlasRegion};
pub use layout::{TextLayout, LayoutOptions, TextAlignment, LineBreakMode, PositionedGlyph};
pub use renderer::{TextRenderer, GlyphInstance, PreparedText};

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
