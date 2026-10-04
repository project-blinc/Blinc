//! Glyph rasterization using swash
//!
//! Converts font glyph outlines to bitmap images for the glyph atlas.
//! Uses swash for high-quality, accurate glyph rendering.
//!
//! Supports both grayscale alpha glyphs (for text) and RGBA color emoji.

use crate::font::FontFace;
use crate::{Result, TextError};
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Format, Vector};

/// Format of the rasterized glyph bitmap
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphFormat {
    /// Single-channel alpha (grayscale)
    Alpha,
    /// RGBA color (for color emoji)
    Rgba,
}

/// Rasterized glyph bitmap with metrics
#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    /// Bitmap pixel data (grayscale 8-bit or RGBA 32-bit)
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
    /// Pixel format of the bitmap
    pub format: GlyphFormat,
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
        self.rasterize_at_offset(font, glyph_id, font_size, 0.0)
    }

    /// Rasterize a glyph shifted `offset_x` of a pixel to the right.
    ///
    /// Used for subpixel horizontal positioning: the same glyph
    /// rasterized at a few fractional offsets lets a caller keep even
    /// letter spacing while still placing every glyph on a whole pixel.
    /// `bearing_x` comes back relative to the shifted raster, so the
    /// caller needs no further correction.
    pub fn rasterize_at_offset(
        &mut self,
        font: &FontFace,
        glyph_id: u16,
        font_size: f32,
        offset_x: f32,
    ) -> Result<RasterizedGlyph> {
        // Get the raw font data and create a swash FontRef with correct face index
        let font_data = font.data();
        let swash_font = swash::FontRef::from_index(font_data, font.face_index() as usize)
            .ok_or(TextError::InvalidFontData)?;

        // Create a scaler for this font at the requested size
        // Same axis the shaper set: a variable face rasterizes at its
        // default instance otherwise, so every weight would draw as
        // regular.
        let mut builder = self.scale_context.builder(swash_font).size(font_size);
        if let Some(w) = font.variation_weight() {
            builder = builder.variations([swash::Setting::from(("wght", w))]);
        }
        let mut scaler = builder.build();

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
        if offset_x != 0.0 {
            render.offset(Vector::new(offset_x, 0.0));
        }

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
                    format: GlyphFormat::Alpha,
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
                    format: GlyphFormat::Alpha,
                })
            }
        }
    }

    /// Rasterize a color emoji glyph as RGBA
    ///
    /// This is used for color emoji fonts like Apple Color Emoji.
    /// Returns RGBA bitmap data suitable for rendering as an image.
    pub fn rasterize_color(
        &mut self,
        font: &FontFace,
        glyph_id: u16,
        font_size: f32,
    ) -> Result<RasterizedGlyph> {
        // Get the raw font data and create a swash FontRef with correct face index
        let font_data = font.data();
        let swash_font = swash::FontRef::from_index(font_data, font.face_index() as usize)
            .ok_or(TextError::InvalidFontData)?;

        // Create a scaler for this font at the requested size
        // Same axis the shaper set: a variable face rasterizes at its
        // default instance otherwise, so every weight would draw as
        // regular.
        let mut builder = self.scale_context.builder(swash_font).size(font_size);
        if let Some(w) = font.variation_weight() {
            builder = builder.variations([swash::Setting::from(("wght", w))]);
        }
        let mut scaler = builder.build();

        // Get advance width from font metrics
        let metrics = swash_font.metrics(&[]);
        let glyph_metrics = swash_font.glyph_metrics(&[]);
        let scale = font_size / metrics.units_per_em as f32;
        let advance = glyph_metrics.advance_width(glyph_id) * scale;

        // Render the glyph - prioritize color bitmap for emoji
        let mut render = Render::new(&[
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::ColorOutline(0),
            Source::Outline,
        ]);

        // Request RGBA format for color emoji
        render.format(Format::Subpixel);

        let image = render.render(&mut scaler, glyph_id);

        match image {
            Some(img) => {
                let bearing_x = img.placement.left;
                let bearing_y = img.placement.top;
                let width = img.placement.width;
                let height = img.placement.height;

                // Check if we got color data (4 bytes per pixel) or grayscale
                let expected_rgba_size = (width * height * 4) as usize;
                let is_color = img.data.len() == expected_rgba_size;

                if is_color {
                    Ok(RasterizedGlyph {
                        bitmap: img.data,
                        width,
                        height,
                        bearing_x: bearing_x as i16,
                        bearing_y: bearing_y as i16,
                        advance: advance.round() as u16,
                        format: GlyphFormat::Rgba,
                    })
                } else {
                    // Fallback: convert grayscale to RGBA
                    let mut rgba = Vec::with_capacity(expected_rgba_size);
                    for alpha in &img.data {
                        rgba.push(255); // R
                        rgba.push(255); // G
                        rgba.push(255); // B
                        rgba.push(*alpha); // A
                    }
                    Ok(RasterizedGlyph {
                        bitmap: rgba,
                        width,
                        height,
                        bearing_x: bearing_x as i16,
                        bearing_y: bearing_y as i16,
                        advance: advance.round() as u16,
                        format: GlyphFormat::Rgba,
                    })
                }
            }
            None => Ok(RasterizedGlyph {
                bitmap: Vec::new(),
                width: 0,
                height: 0,
                bearing_x: 0,
                bearing_y: 0,
                advance: advance.round() as u16,
                format: GlyphFormat::Rgba,
            }),
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
