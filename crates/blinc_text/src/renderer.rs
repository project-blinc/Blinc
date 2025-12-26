//! Text renderer
//!
//! High-level text rendering that combines font loading, shaping,
//! rasterization, atlas management, and glyph instance generation.

use crate::atlas::{GlyphAtlas, GlyphInfo};
use crate::font::FontFace;
use crate::layout::{LayoutOptions, TextLayoutEngine};
use crate::rasterizer::GlyphRasterizer;
use crate::{Result, TextError};
use rustc_hash::FxHashMap;

/// A GPU glyph instance for rendering
#[derive(Debug, Clone, Copy)]
pub struct GlyphInstance {
    /// Position and size in pixels (x, y, width, height)
    pub bounds: [f32; 4],
    /// UV coordinates in atlas (u_min, v_min, u_max, v_max)
    pub uv_bounds: [f32; 4],
    /// Text color (RGBA, 0.0-1.0)
    pub color: [f32; 4],
}

/// Result of preparing text for rendering
#[derive(Debug)]
pub struct PreparedText {
    /// Glyph instances ready for GPU rendering
    pub glyphs: Vec<GlyphInstance>,
    /// Total width of the text
    pub width: f32,
    /// Total height of the text (line height)
    pub height: f32,
    /// Ascender in pixels (distance from baseline to top of em box)
    pub ascender: f32,
    /// Descender in pixels (typically negative, distance from baseline to bottom)
    pub descender: f32,
}

/// Text renderer that manages fonts, atlas, and glyph rendering
pub struct TextRenderer {
    /// Default font
    default_font: Option<FontFace>,
    /// Glyph atlas
    atlas: GlyphAtlas,
    /// Glyph rasterizer
    rasterizer: GlyphRasterizer,
    /// Text layout engine
    layout_engine: TextLayoutEngine,
    /// Cache key: (glyph_id, quantized_size) -> atlas info
    glyph_cache: FxHashMap<(u16, u16), GlyphInfo>,
}

impl TextRenderer {
    /// Create a new text renderer with default atlas size
    pub fn new() -> Self {
        Self {
            default_font: None,
            atlas: GlyphAtlas::default(), // Uses 512x512 for lower memory footprint
            rasterizer: GlyphRasterizer::new(),
            layout_engine: TextLayoutEngine::new(),
            glyph_cache: FxHashMap::default(),
        }
    }

    /// Create with custom atlas size
    pub fn with_atlas_size(width: u32, height: u32) -> Self {
        Self {
            default_font: None,
            atlas: GlyphAtlas::new(width, height),
            rasterizer: GlyphRasterizer::new(),
            layout_engine: TextLayoutEngine::new(),
            glyph_cache: FxHashMap::default(),
        }
    }

    /// Set the default font
    pub fn set_default_font(&mut self, font: FontFace) {
        self.default_font = Some(font);
    }

    /// Load and set the default font from file
    pub fn load_default_font(&mut self, path: &std::path::Path) -> Result<()> {
        let font = FontFace::from_file(path)?;
        self.default_font = Some(font);
        Ok(())
    }

    /// Load and set the default font from data
    pub fn load_default_font_data(&mut self, data: Vec<u8>) -> Result<()> {
        let font = FontFace::from_data(data)?;
        self.default_font = Some(font);
        Ok(())
    }

    /// Get the glyph atlas
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    /// Get mutable atlas (for GPU upload checking)
    pub fn atlas_mut(&mut self) -> &mut GlyphAtlas {
        &mut self.atlas
    }

    /// Check if atlas needs GPU upload
    pub fn atlas_is_dirty(&self) -> bool {
        self.atlas.is_dirty()
    }

    /// Mark atlas as clean after GPU upload
    pub fn mark_atlas_clean(&mut self) {
        self.atlas.mark_clean();
    }

    /// Get atlas pixel data for GPU upload
    pub fn atlas_pixels(&self) -> &[u8] {
        self.atlas.pixels()
    }

    /// Get atlas dimensions
    pub fn atlas_dimensions(&self) -> (u32, u32) {
        self.atlas.dimensions()
    }

    /// Prepare text for rendering, rasterizing glyphs as needed
    pub fn prepare_text(
        &mut self,
        text: &str,
        font_size: f32,
        color: [f32; 4],
        options: &LayoutOptions,
    ) -> Result<PreparedText> {
        // Check font exists
        if self.default_font.is_none() {
            return Err(TextError::FontLoadError("No default font set".to_string()));
        }

        // Get font metrics for the PreparedText result
        let (ascender, descender) = {
            let font = self.default_font.as_ref().unwrap();
            let metrics = font.metrics();
            (
                metrics.ascender_px(font_size),
                metrics.descender_px(font_size),
            )
        };

        // Layout the text - borrow font temporarily
        let layout = {
            let font = self.default_font.as_ref().unwrap();
            self.layout_engine.layout(text, font, font_size, options)
        };

        // Collect positioned glyphs for processing
        let positioned_glyphs: Vec<_> = layout.glyphs().cloned().collect();

        // Convert to GPU glyph instances
        let mut glyphs = Vec::with_capacity(positioned_glyphs.len());
        let atlas_dims = self.atlas.dimensions();

        // First pass: ensure all glyphs are in atlas
        let mut glyph_infos: Vec<Option<GlyphInfo>> = Vec::with_capacity(positioned_glyphs.len());
        for positioned in &positioned_glyphs {
            if positioned.codepoint.is_whitespace() {
                glyph_infos.push(None);
                continue;
            }

            // We need to work around the borrow checker by using unsafe or restructuring.
            // Instead, we'll use a helper function that takes ownership of what it needs.
            let glyph_info = self.rasterize_glyph_if_needed(positioned.glyph_id, font_size)?;
            glyph_infos.push(Some(glyph_info));
        }

        // Second pass: build glyph instances
        for (positioned, glyph_info) in positioned_glyphs.iter().zip(glyph_infos.iter()) {
            let glyph_info = match glyph_info {
                Some(info) => *info,
                None => continue,
            };

            // Skip glyphs with no bitmap (empty glyphs)
            if glyph_info.region.width == 0 || glyph_info.region.height == 0 {
                continue;
            }

            // Calculate screen position
            let x = positioned.x + glyph_info.bearing_x as f32;
            let y = positioned.y - glyph_info.bearing_y as f32;
            let w = glyph_info.region.width as f32;
            let h = glyph_info.region.height as f32;

            // Get UV coordinates
            let uv = glyph_info.region.uv_bounds(atlas_dims.0, atlas_dims.1);

            glyphs.push(GlyphInstance {
                bounds: [x, y, w, h],
                uv_bounds: uv,
                color,
            });
        }

        Ok(PreparedText {
            glyphs,
            width: layout.width,
            height: layout.height,
            ascender,
            descender,
        })
    }

    /// Rasterize a glyph if needed, using the default font
    /// This method avoids borrow checker issues by accessing default_font internally
    fn rasterize_glyph_if_needed(&mut self, glyph_id: u16, font_size: f32) -> Result<GlyphInfo> {
        // Quantize font size for cache key (0.5px granularity)
        let size_key = (font_size * 2.0).round() as u16;
        let cache_key = (glyph_id, size_key);

        // Check cache first (no font borrow needed)
        if let Some(info) = self.glyph_cache.get(&cache_key) {
            return Ok(*info);
        }

        // Check atlas (no font borrow needed)
        if let Some(info) = self.atlas.get_glyph(glyph_id, font_size) {
            self.glyph_cache.insert(cache_key, *info);
            return Ok(*info);
        }

        // Rasterize the glyph - borrow font only for this operation
        let font = self
            .default_font
            .as_ref()
            .ok_or_else(|| TextError::FontLoadError("No default font set".to_string()))?;
        let rasterized = self.rasterizer.rasterize(font, glyph_id, font_size)?;

        // Handle empty glyphs (like space)
        if rasterized.width == 0 || rasterized.height == 0 {
            let info = GlyphInfo {
                region: crate::atlas::AtlasRegion {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                },
                bearing_x: rasterized.bearing_x,
                bearing_y: rasterized.bearing_y,
                advance: rasterized.advance,
                font_size,
            };
            self.glyph_cache.insert(cache_key, info);
            return Ok(info);
        }

        // Insert into atlas
        let info = self.atlas.insert_glyph(
            glyph_id,
            font_size,
            rasterized.width,
            rasterized.height,
            rasterized.bearing_x,
            rasterized.bearing_y,
            rasterized.advance,
            &rasterized.bitmap,
        )?;

        self.glyph_cache.insert(cache_key, info);
        Ok(info)
    }

    /// Clear the glyph cache and atlas
    pub fn clear(&mut self) {
        self.atlas.clear();
        self.glyph_cache.clear();
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}
