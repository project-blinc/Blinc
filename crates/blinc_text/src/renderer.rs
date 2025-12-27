//! Text renderer
//!
//! High-level text rendering that combines font loading, shaping,
//! rasterization, atlas management, and glyph instance generation.

use crate::atlas::{GlyphAtlas, GlyphInfo};
use crate::font::FontFace;
use crate::layout::{LayoutOptions, TextLayoutEngine};
use crate::rasterizer::GlyphRasterizer;
use crate::registry::{FontRegistry, GenericFont};
use crate::{Result, TextError};
use rustc_hash::FxHashMap;
use std::sync::Arc;

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

/// A color span for styled text rendering
#[derive(Debug, Clone, Copy)]
pub struct ColorSpan {
    /// Start byte index in text
    pub start: usize,
    /// End byte index in text (exclusive)
    pub end: usize,
    /// RGBA color
    pub color: [f32; 4],
}

/// Text renderer that manages fonts, atlas, and glyph rendering
pub struct TextRenderer {
    /// Default font (legacy support)
    default_font: Option<FontFace>,
    /// Font registry for system font discovery and caching
    font_registry: FontRegistry,
    /// Glyph atlas
    atlas: GlyphAtlas,
    /// Glyph rasterizer
    rasterizer: GlyphRasterizer,
    /// Text layout engine
    layout_engine: TextLayoutEngine,
    /// Cache key: (font_id, glyph_id, quantized_size) -> atlas info
    /// font_id is hash of font name or 0 for default
    glyph_cache: FxHashMap<(u32, u16, u16), GlyphInfo>,
}

impl TextRenderer {
    /// Create a new text renderer with default atlas size
    pub fn new() -> Self {
        Self {
            default_font: None,
            font_registry: FontRegistry::new(),
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
            font_registry: FontRegistry::new(),
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
        self.prepare_text_internal(text, font_size, color, options, None, GenericFont::System)
    }

    /// Prepare text for rendering with a specific font family
    ///
    /// # Arguments
    /// * `text` - The text to render
    /// * `font_size` - Font size in pixels
    /// * `color` - Text color (RGBA, 0.0-1.0)
    /// * `options` - Layout options
    /// * `font_name` - Optional font name (e.g., "Fira Code", "Inter")
    /// * `generic` - Generic font fallback category
    pub fn prepare_text_with_font(
        &mut self,
        text: &str,
        font_size: f32,
        color: [f32; 4],
        options: &LayoutOptions,
        font_name: Option<&str>,
        generic: GenericFont,
    ) -> Result<PreparedText> {
        self.prepare_text_internal(text, font_size, color, options, font_name, generic)
    }

    /// Internal method for preparing text with optional font family
    fn prepare_text_internal(
        &mut self,
        text: &str,
        font_size: f32,
        color: [f32; 4],
        options: &LayoutOptions,
        font_name: Option<&str>,
        generic: GenericFont,
    ) -> Result<PreparedText> {
        // Resolve the font to use
        let font = self.resolve_font(font_name, generic)?;
        let font_id = self.font_id(font_name, generic);

        // Get font metrics for the PreparedText result
        let (ascender, descender) = {
            let metrics = font.metrics();
            (
                metrics.ascender_px(font_size),
                metrics.descender_px(font_size),
            )
        };

        // Layout the text
        let layout = self.layout_engine.layout(text, &font, font_size, options);

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

            let glyph_info =
                self.rasterize_glyph_for_font(&font, font_id, positioned.glyph_id, font_size)?;
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
            // positioned.x is the pen position from the shaper (includes advance)
            // bearing_x is the offset from pen position to the glyph's left edge
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

    /// Prepare styled text with multiple color spans
    ///
    /// This renders text as a single unit but applies different colors to different ranges.
    /// Unlike creating separate text elements, this ensures proper character spacing.
    pub fn prepare_styled_text(
        &mut self,
        text: &str,
        font_size: f32,
        default_color: [f32; 4],
        color_spans: &[ColorSpan],
        options: &LayoutOptions,
        font_name: Option<&str>,
        generic: GenericFont,
    ) -> Result<PreparedText> {
        // Resolve the font to use
        let font = self.resolve_font(font_name, generic)?;
        let font_id = self.font_id(font_name, generic);

        // Get font metrics
        let (ascender, descender) = {
            let metrics = font.metrics();
            (
                metrics.ascender_px(font_size),
                metrics.descender_px(font_size),
            )
        };

        // Layout the text (this gives us proper positions from HarfBuzz)
        let layout = self.layout_engine.layout(text, &font, font_size, options);

        // Collect positioned glyphs
        let positioned_glyphs: Vec<_> = layout.glyphs().cloned().collect();

        // Build a map of byte position to color
        // For each character, find which span it belongs to
        let get_color_for_byte_pos = |byte_pos: usize| -> [f32; 4] {
            for span in color_spans {
                if byte_pos >= span.start && byte_pos < span.end {
                    return span.color;
                }
            }
            default_color
        };

        // Convert to GPU glyph instances
        let mut glyphs = Vec::with_capacity(positioned_glyphs.len());
        let atlas_dims = self.atlas.dimensions();

        // First pass: rasterize all glyphs
        let mut glyph_infos: Vec<Option<GlyphInfo>> = Vec::with_capacity(positioned_glyphs.len());
        for positioned in &positioned_glyphs {
            if positioned.codepoint.is_whitespace() {
                glyph_infos.push(None);
                continue;
            }

            let glyph_info =
                self.rasterize_glyph_for_font(&font, font_id, positioned.glyph_id, font_size)?;
            glyph_infos.push(Some(glyph_info));
        }

        // Second pass: build glyph instances with per-glyph colors
        // We need to map glyph cluster (byte position) to color
        let byte_positions: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();

        for (i, (positioned, glyph_info)) in positioned_glyphs.iter().zip(glyph_infos.iter()).enumerate() {
            let glyph_info = match glyph_info {
                Some(info) => *info,
                None => continue,
            };

            if glyph_info.region.width == 0 || glyph_info.region.height == 0 {
                continue;
            }

            // Get the byte position for this glyph's cluster to determine color
            let byte_pos = byte_positions.get(i).copied().unwrap_or(0);
            let color = get_color_for_byte_pos(byte_pos);

            // positioned.x is the pen position from the shaper
            // bearing_x is the offset from pen position to the glyph's left edge
            let x = positioned.x + glyph_info.bearing_x as f32;
            let y = positioned.y - glyph_info.bearing_y as f32;
            let w = glyph_info.region.width as f32;
            let h = glyph_info.region.height as f32;

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

    /// Resolve font by name or generic category, with fallback to default
    /// Uses only cached fonts - fonts should be preloaded at app startup
    fn resolve_font(
        &self,
        font_name: Option<&str>,
        generic: GenericFont,
    ) -> Result<Arc<FontFace>> {
        // Use cache-only lookup - fonts should be preloaded at startup
        if let Some(font) = self.font_registry.get_for_render(font_name, generic) {
            return Ok(font);
        }

        // If named font not found, fall back to generic (which should always be cached)
        if font_name.is_some() {
            if let Some(font) = self.font_registry.get_cached_generic(generic) {
                return Ok(font);
            }
            // Ultimate fallback to SansSerif
            if let Some(font) = self.font_registry.get_cached_generic(GenericFont::SansSerif) {
                return Ok(font);
            }
        }

        Err(TextError::FontLoadError(
            "No fonts available - fonts should be preloaded at startup".to_string()
        ))
    }

    /// Preload fonts that your app uses (call at startup)
    pub fn preload_fonts(&mut self, names: &[&str]) {
        self.font_registry.preload_fonts(names);
    }

    /// Generate a unique font ID for cache keys
    fn font_id(&self, font_name: Option<&str>, generic: GenericFont) -> u32 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        font_name.hash(&mut hasher);
        generic.hash(&mut hasher);
        hasher.finish() as u32
    }

    /// Rasterize a glyph for a specific font
    fn rasterize_glyph_for_font(
        &mut self,
        font: &FontFace,
        font_id: u32,
        glyph_id: u16,
        font_size: f32,
    ) -> Result<GlyphInfo> {
        // Quantize font size for cache key (0.5px granularity)
        let size_key = (font_size * 2.0).round() as u16;
        let cache_key = (font_id, glyph_id, size_key);

        // Check cache first
        if let Some(info) = self.glyph_cache.get(&cache_key) {
            return Ok(*info);
        }

        // Rasterize the glyph
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
            font_id,
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

    /// Legacy method for backward compatibility - uses system font from registry
    #[allow(dead_code)]
    fn rasterize_glyph_if_needed(&mut self, glyph_id: u16, font_size: f32) -> Result<GlyphInfo> {
        let font = self.font_registry.load_generic(GenericFont::SansSerif)?;
        self.rasterize_glyph_for_font(&font, 0, glyph_id, font_size)
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
