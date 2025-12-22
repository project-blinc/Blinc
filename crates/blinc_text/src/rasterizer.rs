//! Glyph rasterization
//!
//! Converts font glyph outlines to bitmap images for the glyph atlas.

use crate::font::FontFace;
use crate::{Result, TextError};
use ab_glyph_rasterizer::{point, Point, Rasterizer};

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

/// Glyph rasterizer using ttf-parser outlines
pub struct GlyphRasterizer {
    /// Padding around each glyph (for anti-aliasing/SDF)
    padding: u32,
}

impl GlyphRasterizer {
    /// Create a new glyph rasterizer
    pub fn new() -> Self {
        Self { padding: 1 }
    }

    /// Create with custom padding
    pub fn with_padding(padding: u32) -> Self {
        Self { padding }
    }

    /// Rasterize a glyph at the given font size
    pub fn rasterize(
        &self,
        font: &FontFace,
        glyph_id: u16,
        font_size: f32,
    ) -> Result<RasterizedGlyph> {
        let face = font
            .as_ttf_face()
            .ok_or_else(|| TextError::InvalidFontData)?;

        let glyph_id = ttf_parser::GlyphId(glyph_id);

        // Get glyph bounding box
        let bbox = face.glyph_bounding_box(glyph_id);

        // Get horizontal advance
        let advance = face.glyph_hor_advance(glyph_id).unwrap_or(0);

        // Calculate scale factor
        let units_per_em = face.units_per_em() as f32;
        let scale = font_size / units_per_em;

        // Handle empty glyphs (like space)
        let bbox = match bbox {
            Some(b) => b,
            None => {
                return Ok(RasterizedGlyph {
                    bitmap: Vec::new(),
                    width: 0,
                    height: 0,
                    bearing_x: 0,
                    bearing_y: 0,
                    advance: (advance as f32 * scale).round() as u16,
                });
            }
        };

        // Calculate pixel dimensions with padding
        let x_min = (bbox.x_min as f32 * scale).floor() as i32 - self.padding as i32;
        let y_min = (bbox.y_min as f32 * scale).floor() as i32 - self.padding as i32;
        let x_max = (bbox.x_max as f32 * scale).ceil() as i32 + self.padding as i32;
        let y_max = (bbox.y_max as f32 * scale).ceil() as i32 + self.padding as i32;

        let width = (x_max - x_min) as u32;
        let height = (y_max - y_min) as u32;

        // Sanity check
        if width == 0 || height == 0 || width > 1024 || height > 1024 {
            return Ok(RasterizedGlyph {
                bitmap: Vec::new(),
                width: 0,
                height: 0,
                bearing_x: 0,
                bearing_y: 0,
                advance: (advance as f32 * scale).round() as u16,
            });
        }

        // Create rasterizer
        let mut rasterizer = Rasterizer::new(width as usize, height as usize);

        // Build outline using ttf-parser's outline builder
        let mut builder = OutlineBuilder {
            rasterizer: &mut rasterizer,
            scale,
            offset_x: -x_min as f32,
            offset_y: y_max as f32, // Flip Y axis
            last_point: point(0.0, 0.0),
        };

        face.outline_glyph(glyph_id, &mut builder);

        // Rasterize to bitmap
        let mut bitmap = vec![0u8; (width * height) as usize];
        rasterizer.for_each_pixel_2d(|x, y, alpha| {
            let idx = y as usize * width as usize + x as usize;
            if idx < bitmap.len() {
                bitmap[idx] = (alpha * 255.0) as u8;
            }
        });

        Ok(RasterizedGlyph {
            bitmap,
            width,
            height,
            bearing_x: x_min as i16,
            bearing_y: y_max as i16,
            advance: (advance as f32 * scale).round() as u16,
        })
    }
}

impl Default for GlyphRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Outline builder that feeds path commands to ab_glyph_rasterizer
struct OutlineBuilder<'a> {
    rasterizer: &'a mut Rasterizer,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    last_point: Point,
}

impl OutlineBuilder<'_> {
    fn transform(&self, x: f32, y: f32) -> Point {
        point(
            x * self.scale + self.offset_x,
            self.offset_y - y * self.scale, // Flip Y
        )
    }
}

impl ttf_parser::OutlineBuilder for OutlineBuilder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.last_point = self.transform(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.transform(x, y);
        self.rasterizer.draw_line(self.last_point, p);
        self.last_point = p;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p1 = self.transform(x1, y1);
        let p = self.transform(x, y);
        self.rasterizer.draw_quad(self.last_point, p1, p);
        self.last_point = p;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p1 = self.transform(x1, y1);
        let p2 = self.transform(x2, y2);
        let p = self.transform(x, y);
        self.rasterizer.draw_cubic(self.last_point, p1, p2, p);
        self.last_point = p;
    }

    fn close(&mut self) {
        // ab_glyph_rasterizer auto-closes paths via fill rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rasterizer_creation() {
        let rasterizer = GlyphRasterizer::new();
        assert_eq!(rasterizer.padding, 1);
    }
}
