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
            return Err(SvgError::Parse(
                "Invalid dimensions: width and height must be > 0".into(),
            ));
        }

        // Trim leading whitespace - XML declaration must be at start if present
        let data = {
            let s = std::str::from_utf8(data).unwrap_or("");
            s.trim_start().as_bytes()
        };

        // Parse SVG
        let options = Options::default();
        let tree = Tree::from_data(data, &options).map_err(|e| SvgError::Parse(e.to_string()))?;

        Self::from_tree(&tree, width, height)
    }

    /// Rasterize a parsed usvg Tree to the specified size
    pub fn from_tree(tree: &Tree, width: u32, height: u32) -> Result<Self, SvgError> {
        if width == 0 || height == 0 {
            return Err(SvgError::Parse(
                "Invalid dimensions: width and height must be > 0".into(),
            ));
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

        let transform = Transform::from_scale(scale, scale).post_translate(offset_x, offset_y);

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
                assert_eq!(chunk[0], 0); // R = 0
                assert_eq!(chunk[1], 255); // G = 255 (1.0 * 1.0 * 255)
                assert_eq!(chunk[2], 0); // B = 0
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

    #[test]
    fn test_complex_svg_with_transform() {
        let svg = r#"
<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg"
     viewBox="0 0 240 179.2"
     width="600"
     height="448">
  <title>Fennec Fox Silhouette</title>
  <g transform="translate(0, 179.2) scale(0.01, -0.01)" fill="white">
    <path d="M5385 15919 c-230 -29 -420 -190 -559 -474 -94 -191 -143 -357 -196
-665 -32 -190 -82 -570 -100 -770 -13 -148 -27 -563 -23 -670 7 -165 44 -663
58 -780 44 -370 151 -910 241 -1210 108 -363 247 -690 474 -1120 63 -120 64
-123 106 -375 49 -296 76 -396 149 -555 34 -74 84 -198 110 -275 134 -397 200
-523 370 -703 89 -94 193 -180 390 -317 149 -104 267 -199 326 -262 70 -73
186 -256 233 -368 134 -315 146 -336 267 -469 93 -103 174 -214 204 -279 21
-43 25 -68 25 -142 0 -79 -5 -104 -40 -204 -41 -117 -48 -167 -30 -202 9 -16
22 -19 83 -19 44 0 78 -5 85 -12 20 -20 14 -62 -23 -163 -63 -174 -50 -206 93
-226 92 -13 101 -35 57 -131 -14 -29 -25 -67 -25 -84 0 -30 4 -34 68 -59 97
-40 185 -82 202 -97 35 -29 69 -86 85 -142 17 -58 17 -109 -1 -241 -8 -67 14
-90 253 -248 236 -157 335 -213 694 -391 129 -64 259 -130 289 -147 30 -16 89
-48 130 -71 112 -60 239 -137 295 -178 28 -20 102 -74 165 -120 249 -181 360
-288 427 -410 139 -255 364 -463 606 -561 48 -20 69 -36 112 -90 105 -130 252
-244 384 -299 143 -58 260 -81 447 -87 420 -13 688 92 910 357 65 77 71 82
198 144 100 49 151 81 217 138 165 142 274 275 344 423 41 86 166 205 375 357
324 234 529 354 985 577 235 115 466 245 630 354 108 72 245 188 251 213 4 13
1 54 -6 90 -31 161 29 308 149 368 25 13 80 36 121 50 41 15 78 33 82 39 15
23 8 72 -17 125 -42 90 -35 104 66 125 35 8 79 22 98 31 33 18 33 19 28 69 -3
29 -22 87 -41 129 -69 148 -63 168 53 168 42 0 82 4 90 9 22 14 8 111 -34 236
-34 102 -37 117 -33 200 6 127 41 186 253 430 106 122 117 141 201 335 81 188
104 236 161 322 96 145 228 265 508 460 284 198 478 413 575 637 20 47 69 179
108 293 39 114 94 258 123 320 88 194 98 235 155 598 l27 175 132 265 c180
361 223 457 310 696 99 269 153 463 219 774 75 353 129 745 168 1225 16 204
16 787 0 945 -61 591 -131 998 -219 1265 -104 319 -308 572 -518 645 -304 106
-688 -34 -1150 -419 -324 -271 -785 -736 -1189 -1201 -212 -244 -444 -505
-641 -720 -165 -182 -438 -495 -629 -724 -170 -203 -318 -366 -610 -671 -148
-154 -376 -396 -507 -537 -131 -141 -259 -273 -284 -294 -62 -51 -117 -56
-206 -20 -119 47 -164 31 -272 -101 -64 -79 -108 -116 -165 -140 -39 -16 -117
-14 -297 7 -44 5 -136 16 -205 25 -351 42 -591 39 -1010 -11 -124 -15 -263
-28 -310 -28 -119 -1 -155 18 -260 140 -45 53 -96 104 -114 115 -43 26 -103
24 -168 -6 -29 -14 -67 -25 -84 -25 -75 0 -178 82 -324 260 -68 83 -370 405
-725 773 -251 260 -308 322 -458 502 -167 200 -328 385 -597 685 -91 102 -192
217 -225 255 -33 39 -103 117 -155 175 -52 58 -153 173 -225 255 -311 360
-502 566 -754 813 -152 149 -412 391 -516 478 -194 164 -449 324 -627 395
-109 43 -274 63 -393 48z"/>
  </g>
</svg>
        "#;

        let result = RasterizedSvg::from_str(svg, 64, 64);
        match &result {
            Ok(r) => {
                assert_eq!(r.width, 64);
                assert_eq!(r.height, 64);
            }
            Err(e) => panic!("Failed to rasterize complex SVG: {}", e),
        }
    }
}
