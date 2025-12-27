//! Glyph rasterization using swash
//!
//! Converts font glyph outlines to bitmap images for the glyph atlas.
//! Uses swash for high-quality, accurate glyph rendering.

use crate::font::FontFace;
use crate::{Result, TextError};
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;

/// Rasterized glyph bitmap with metrics
#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    /// Bitmap pixel data (grayscale, 8-bit)
    pub bitmap: Vec<u8>,
    /// Bitmap width in pixels
    pub width: u32,
    /// Bitmap height in pixels
    pub height: u32,
    /// Horizontal bearing (offset from origin to left edge)
    pub bearing_x: i16,
    /// Vertical bearing (offset from baseline to top edge)
    pub bearing_y: i16,
    /// Horizontal advance to next glyph position
    pub advance: u16,
}

/// Glyph rasterizer using swash
pub struct GlyphRasterizer {
    /// Swash scale context (caches scaling state)
    scale_context: ScaleContext,
}

impl GlyphRasterizer {
    /// Create a new glyph rasterizer
    pub fn new() -> Self {
        Self {
            scale_context: ScaleContext::new(),
        }
    }

    /// Rasterize a glyph at the given font size
    pub fn rasterize(
        &mut self,
        font: &FontFace,
        glyph_id: u16,
        font_size: f32,
    ) -> Result<RasterizedGlyph> {
        // Get the raw font data and create a swash FontRef with correct face index
        let font_data = font.data();
        let swash_font = swash::FontRef::from_index(font_data, font.face_index() as usize)
            .ok_or_else(|| TextError::InvalidFontData)?;

        // Create a scaler for this font at the requested size
        let mut scaler = self.scale_context
            .builder(swash_font)
            .size(font_size)
            .build();

        // Get advance width from font metrics (scale from font units to pixels)
        let metrics = swash_font.metrics(&[]);
        let glyph_metrics = swash_font.glyph_metrics(&[]);
        let scale = font_size / metrics.units_per_em as f32;

        // Get advance width for this glyph (already in font units)
        let advance = glyph_metrics.advance_width(glyph_id) * scale;

        // Render the glyph
        let mut render = Render::new(&[
            // Use alpha mask (grayscale) rendering
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ]);

        // Set the format to alpha (8-bit grayscale)
        render.format(Format::Alpha);

        // Render the glyph
        let image = render.render(&mut scaler, glyph_id);

        match image {
            Some(img) => {
                // Extract placement (bearing) information
                let bearing_x = img.placement.left;
                let bearing_y = img.placement.top;
                let width = img.placement.width;
                let height = img.placement.height;

                Ok(RasterizedGlyph {
                    bitmap: img.data,
                    width,
                    height,
                    bearing_x: bearing_x as i16,
                    bearing_y: bearing_y as i16,
                    advance: advance.round() as u16,
                })
            }
            None => {
                // Empty glyph (like space) - no bitmap but has advance
                Ok(RasterizedGlyph {
                    bitmap: Vec::new(),
                    width: 0,
                    height: 0,
                    bearing_x: 0,
                    bearing_y: 0,
                    advance: advance.round() as u16,
                })
            }
        }
    }
}

impl Default for GlyphRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rasterizer_creation() {
        let _rasterizer = GlyphRasterizer::new();
    }
}
