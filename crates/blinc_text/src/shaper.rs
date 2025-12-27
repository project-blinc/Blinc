//! Text shaping using rustybuzz (HarfBuzz)
//!
//! Converts text strings into positioned glyph sequences with proper
//! kerning, ligatures, and OpenType feature support.

use crate::font::FontFace;
use rustybuzz::{Face, UnicodeBuffer};

/// A shaped glyph with position information
#[derive(Debug, Clone, Copy)]
pub struct ShapedGlyph {
    /// Glyph ID in the font
    pub glyph_id: u16,
    /// Character this glyph represents (for debugging/fallback)
    pub codepoint: char,
    /// X offset from current position (for combining marks, etc.)
    pub x_offset: i32,
    /// Y offset from current position
    pub y_offset: i32,
    /// Horizontal advance to next glyph position
    pub x_advance: i32,
    /// Vertical advance (typically 0 for horizontal text)
    pub y_advance: i32,
    /// Index in the original string (cluster)
    pub cluster: u32,
}

/// Result of shaping a text string
#[derive(Debug, Clone)]
pub struct ShapedText {
    /// Shaped glyphs in visual order
    pub glyphs: Vec<ShapedGlyph>,
    /// Total advance width in font units
    pub total_advance: i32,
    /// Font size used for shaping reference
    pub font_size: f32,
    /// Units per em from the font
    pub units_per_em: u16,
}

impl ShapedText {
    /// Get total width in pixels
    pub fn width_px(&self) -> f32 {
        self.total_advance as f32 * self.font_size / self.units_per_em as f32
    }

    /// Scale a font-unit value to pixels
    pub fn scale(&self, value: i32) -> f32 {
        value as f32 * self.font_size / self.units_per_em as f32
    }
}

/// Text shaper using HarfBuzz via rustybuzz
pub struct TextShaper {
    // We don't cache the Face here because it borrows from FontFace data
    // Instead, create it on-demand in shape()
}

impl TextShaper {
    /// Create a new text shaper
    pub fn new() -> Self {
        Self {}
    }

    /// Shape a text string using the given font
    pub fn shape(&self, text: &str, font_face: &FontFace, font_size: f32) -> ShapedText {
        // Create rustybuzz Face from font data with correct face index
        let face = match Face::from_slice(font_face.data(), font_face.face_index()) {
            Some(f) => f,
            None => {
                // Fallback: return basic glyph sequence without shaping
                return self.fallback_shape(text, font_face, font_size);
            }
        };

        // Create and fill the Unicode buffer
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);

        // Shape the buffer
        let output = rustybuzz::shape(&face, &[], buffer);

        // Extract glyph information
        let glyph_infos = output.glyph_infos();
        let glyph_positions = output.glyph_positions();

        let mut glyphs = Vec::with_capacity(glyph_infos.len());
        let mut total_advance = 0i32;

        for (info, pos) in glyph_infos.iter().zip(glyph_positions.iter()) {
            // Find the original character for this cluster
            let codepoint = text
                .char_indices()
                .find(|(i, _)| *i as u32 == info.cluster)
                .map(|(_, c)| c)
                .unwrap_or('\u{FFFD}');

            glyphs.push(ShapedGlyph {
                glyph_id: info.glyph_id as u16,
                codepoint,
                x_offset: pos.x_offset,
                y_offset: pos.y_offset,
                x_advance: pos.x_advance,
                y_advance: pos.y_advance,
                cluster: info.cluster,
            });

            total_advance += pos.x_advance;
        }

        ShapedText {
            glyphs,
            total_advance,
            font_size,
            units_per_em: font_face.metrics().units_per_em,
        }
    }

    /// Fallback shaping when rustybuzz fails
    fn fallback_shape(&self, text: &str, font_face: &FontFace, font_size: f32) -> ShapedText {
        let mut glyphs = Vec::new();
        let mut total_advance = 0i32;

        for (cluster, c) in text.char_indices() {
            let glyph_id = font_face.glyph_id(c).unwrap_or(0);
            let advance = font_face.glyph_advance(glyph_id).unwrap_or(500) as i32;

            glyphs.push(ShapedGlyph {
                glyph_id,
                codepoint: c,
                x_offset: 0,
                y_offset: 0,
                x_advance: advance,
                y_advance: 0,
                cluster: cluster as u32,
            });

            total_advance += advance;
        }

        ShapedText {
            glyphs,
            total_advance,
            font_size,
            units_per_em: font_face.metrics().units_per_em,
        }
    }

    /// Shape with specific OpenType features enabled/disabled
    pub fn shape_with_features(
        &self,
        text: &str,
        font_face: &FontFace,
        font_size: f32,
        features: &[rustybuzz::Feature],
    ) -> ShapedText {
        let face = match Face::from_slice(font_face.data(), font_face.face_index()) {
            Some(f) => f,
            None => return self.fallback_shape(text, font_face, font_size),
        };

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);

        let output = rustybuzz::shape(&face, features, buffer);

        let glyph_infos = output.glyph_infos();
        let glyph_positions = output.glyph_positions();

        let mut glyphs = Vec::with_capacity(glyph_infos.len());
        let mut total_advance = 0i32;

        for (info, pos) in glyph_infos.iter().zip(glyph_positions.iter()) {
            let codepoint = text
                .char_indices()
                .find(|(i, _)| *i as u32 == info.cluster)
                .map(|(_, c)| c)
                .unwrap_or('\u{FFFD}');

            glyphs.push(ShapedGlyph {
                glyph_id: info.glyph_id as u16,
                codepoint,
                x_offset: pos.x_offset,
                y_offset: pos.y_offset,
                x_advance: pos.x_advance,
                y_advance: pos.y_advance,
                cluster: info.cluster,
            });

            total_advance += pos.x_advance;
        }

        ShapedText {
            glyphs,
            total_advance,
            font_size,
            units_per_em: font_face.metrics().units_per_em,
        }
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}
