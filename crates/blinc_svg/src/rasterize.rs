//! SVG rasterization using resvg for high-quality anti-aliased output
//!
//! This module provides CPU-based SVG rasterization using resvg and tiny-skia,
//! producing pixel-perfect anti-aliased output that can be uploaded as GPU textures.

use tiny_skia::{Pixmap, Transform};
use usvg::{Options, Tree};

use crate::error::SvgError;

/// Rasterized SVG image data
pub struct RasterizedSvg {
    /// RGBA pixel data (premultiplied alpha)
    pub pixels: Vec<u8>,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
}

impl RasterizedSvg {
    /// Rasterize an SVG string to the specified size
    ///
    /// The SVG will be scaled to fit within the given dimensions while
    /// maintaining aspect ratio, centered within the bounds.
    pub fn from_str(svg_str: &str, width: u32, height: u32) -> Result<Self, SvgError> {
        Self::from_data(svg_str.as_bytes(), width, height)
    }

    /// Rasterize SVG data to the specified size
    pub fn from_data(data: &[u8], width: u32, height: u32) -> Result<Self, SvgError> {
        if width == 0 || height == 0 {
            return Err(SvgError::Parse("Invalid dimensions: width and height must be > 0".into()));
        }

        // Parse SVG
        let options = Options::default();
        let tree = Tree::from_data(data, &options)
            .map_err(|e| SvgError::Parse(e.to_string()))?;

        Self::from_tree(&tree, width, height)
    }

    /// Rasterize a parsed usvg Tree to the specified size
    pub fn from_tree(tree: &Tree, width: u32, height: u32) -> Result<Self, SvgError> {
        if width == 0 || height == 0 {
            return Err(SvgError::Parse("Invalid dimensions: width and height must be > 0".into()));
        }

        // Create pixmap
        let mut pixmap = Pixmap::new(width, height)
            .ok_or_else(|| SvgError::Parse("Failed to create pixmap".into()))?;

        // Calculate transform to fit SVG in bounds while maintaining aspect ratio
        let svg_size = tree.size();
        let scale_x = width as f32 / svg_size.width();
        let scale_y = height as f32 / svg_size.height();
        let scale = scale_x.min(scale_y);

        // Center the SVG within the bounds
        let scaled_width = svg_size.width() * scale;
        let scaled_height = svg_size.height() * scale;
        let offset_x = (width as f32 - scaled_width) / 2.0;
        let offset_y = (height as f32 - scaled_height) / 2.0;

        let transform = Transform::from_scale(scale, scale)
            .post_translate(offset_x, offset_y);

        // Render
        resvg::render(tree, transform, &mut pixmap.as_mut());

        // Convert from premultiplied alpha to straight alpha for GPU upload
        let pixels = unpremultiply_alpha(pixmap.data());

        Ok(Self {
            pixels,
            width,
            height,
        })
    }

    /// Rasterize an SVG string with a tint color applied
    ///
    /// This renders the SVG and then applies the tint color to all non-transparent pixels.
    pub fn from_str_with_tint(
        svg_str: &str,
        width: u32,
        height: u32,
        tint: blinc_core::Color,
    ) -> Result<Self, SvgError> {
        let mut rasterized = Self::from_str(svg_str, width, height)?;
        rasterized.apply_tint(tint);
        Ok(rasterized)
    }

    /// Apply a tint color to the rasterized image
    ///
    /// This replaces the RGB values of all pixels with the tint color,
    /// while preserving the original alpha channel. Output is premultiplied
    /// alpha for correct GPU blending.
    pub fn apply_tint(&mut self, tint: blinc_core::Color) {
        for chunk in self.pixels.chunks_exact_mut(4) {
            let alpha = chunk[3] as f32 / 255.0;
            if alpha > 0.0 {
                // Calculate final alpha (original alpha * tint alpha)
                let final_a = alpha * tint.a;
                // Output premultiplied alpha: RGB = tint_rgb * final_alpha
                // This ensures correct GPU blending with standard blend equations
                chunk[0] = ((tint.r * final_a) * 255.0).clamp(0.0, 255.0) as u8;
                chunk[1] = ((tint.g * final_a) * 255.0).clamp(0.0, 255.0) as u8;
                chunk[2] = ((tint.b * final_a) * 255.0).clamp(0.0, 255.0) as u8;
                chunk[3] = (final_a * 255.0).clamp(0.0, 255.0) as u8;
            }
        }
    }

    /// Get the pixel data as a slice
    pub fn data(&self) -> &[u8] {
        &self.pixels
    }

    /// Get the dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Convert premultiplied alpha to straight alpha
///
/// tiny-skia outputs premultiplied alpha, but most GPU texture formats
/// expect straight alpha.
fn unpremultiply_alpha(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());

    for chunk in data.chunks_exact(4) {
        let a = chunk[3] as f32 / 255.0;
        if a > 0.0 {
            // Unpremultiply: RGB = RGB_premul / A
            let r = ((chunk[0] as f32 / a).min(255.0)) as u8;
            let g = ((chunk[1] as f32 / a).min(255.0)) as u8;
            let b = ((chunk[2] as f32 / a).min(255.0)) as u8;
            result.extend_from_slice(&[r, g, b, chunk[3]]);
        } else {
            result.extend_from_slice(&[0, 0, 0, 0]);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rasterize_simple_svg() {
        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24">
                <circle cx="12" cy="12" r="10" fill="red"/>
            </svg>
        "#;

        let rasterized = RasterizedSvg::from_str(svg, 48, 48).unwrap();
        assert_eq!(rasterized.width, 48);
        assert_eq!(rasterized.height, 48);
        assert_eq!(rasterized.pixels.len(), 48 * 48 * 4);
    }

    #[test]
    fn test_rasterize_with_tint() {
        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24">
                <circle cx="12" cy="12" r="10" fill="white"/>
            </svg>
        "#;

        let tint = blinc_core::Color::rgba(0.0, 1.0, 0.0, 1.0); // Green with full alpha
        let rasterized = RasterizedSvg::from_str_with_tint(svg, 24, 24, tint).unwrap();

        // Find a fully opaque pixel and verify it's green (premultiplied)
        for chunk in rasterized.pixels.chunks_exact(4) {
            if chunk[3] == 255 {
                // Fully opaque pixel: premultiplied green should be (0, 255, 0, 255)
                assert_eq!(chunk[0], 0);   // R = 0
                assert_eq!(chunk[1], 255); // G = 255 (1.0 * 1.0 * 255)
                assert_eq!(chunk[2], 0);   // B = 0
                break;
            }
        }
    }

    #[test]
    fn test_zero_dimensions_error() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"></svg>"#;
        assert!(RasterizedSvg::from_str(svg, 0, 24).is_err());
        assert!(RasterizedSvg::from_str(svg, 24, 0).is_err());
    }
}
