//! Text renderer
//!
//! High-level text rendering that combines font loading, shaping,
//! rasterization, atlas management, and glyph instance generation.
//!
//! Supports automatic emoji font fallback - when the primary font doesn't
//! have a glyph for an emoji character, the system emoji font is used.

use crate::atlas::{ColorGlyphAtlas, GlyphAtlas, GlyphInfo};
use crate::emoji::{is_emoji, is_variation_selector, is_zwj};
use crate::font::FontFace;
use crate::layout::{LayoutOptions, PositionedGlyph, TextLayoutEngine};
use crate::rasterizer::GlyphRasterizer;
use crate::registry::{FontRegistry, GenericFont};
use crate::shaper::TextShaper;
use crate::{Result, TextError};
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;

/// Maximum number of glyphs to keep in the grayscale glyph cache
const GLYPH_CACHE_CAPACITY: usize = 2048;

/// Maximum number of color glyphs (emoji) to keep in cache
const COLOR_GLYPH_CACHE_CAPACITY: usize = 512;

/// A GPU glyph instance for rendering
#[derive(Debug, Clone, Copy)]
pub struct GlyphInstance {
    /// Position and size in pixels (x, y, width, height)
    pub bounds: [f32; 4],
    /// UV coordinates in atlas (u_min, v_min, u_max, v_max)
    pub uv_bounds: [f32; 4],
    /// Text color (RGBA, 0.0-1.0)
    pub color: [f32; 4],
    /// Whether this glyph is from the color atlas (emoji)
    pub is_color: bool,
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
    /// Can be shared with other components (like text measurement)
    font_registry: Arc<std::sync::Mutex<FontRegistry>>,
    /// Glyph atlas (grayscale for regular text)
    atlas: GlyphAtlas,
    /// Color glyph atlas (RGBA for color emoji)
    color_atlas: ColorGlyphAtlas,
    /// Glyph rasterizer
    rasterizer: GlyphRasterizer,
    /// Text layout engine
    layout_engine: TextLayoutEngine,
    /// LRU cache for grayscale glyphs: (font_id, glyph_id, quantized_size) -> atlas info
    /// font_id is hash of font name or 0 for default
    glyph_cache: LruCache<(u32, u16, u16), GlyphInfo>,
    /// LRU cache for color glyphs (emoji) - same key format
    color_glyph_cache: LruCache<(u32, u16, u16), GlyphInfo>,
}

impl TextRenderer {
    /// Create a new text renderer with default atlas size.
    ///
    /// Uses the global shared font registry to minimize memory usage.
    /// Apple Color Emoji alone is 180MB - sharing prevents loading it multiple times.
    pub fn new() -> Self {
        Self {
            default_font: None,
            font_registry: crate::global_font_registry(),
            atlas: GlyphAtlas::default(),
            color_atlas: ColorGlyphAtlas::default(),
            rasterizer: GlyphRasterizer::new(),
            layout_engine: TextLayoutEngine::new(),
            glyph_cache: LruCache::new(NonZeroUsize::new(GLYPH_CACHE_CAPACITY).unwrap()),
            color_glyph_cache: LruCache::new(
                NonZeroUsize::new(COLOR_GLYPH_CACHE_CAPACITY).unwrap(),
            ),
        }
    }

    /// Create a new text renderer with a shared font registry
    ///
    /// Use this to share fonts between text measurement and rendering,
    /// ensuring consistent text layout.
    pub fn with_shared_registry(registry: Arc<std::sync::Mutex<FontRegistry>>) -> Self {
        Self {
            default_font: None,
            font_registry: registry,
            atlas: GlyphAtlas::default(),
            color_atlas: ColorGlyphAtlas::default(),
            rasterizer: GlyphRasterizer::new(),
            layout_engine: TextLayoutEngine::new(),
            glyph_cache: LruCache::new(NonZeroUsize::new(GLYPH_CACHE_CAPACITY).unwrap()),
            color_glyph_cache: LruCache::new(
                NonZeroUsize::new(COLOR_GLYPH_CACHE_CAPACITY).unwrap(),
            ),
        }
    }

    /// Create with custom atlas size.
    ///
    /// Uses the global shared font registry to minimize memory usage.
    pub fn with_atlas_size(width: u32, height: u32) -> Self {
        Self {
            default_font: None,
            font_registry: crate::global_font_registry(),
            atlas: GlyphAtlas::new(width, height),
            color_atlas: ColorGlyphAtlas::default(),
            rasterizer: GlyphRasterizer::new(),
            layout_engine: TextLayoutEngine::new(),
            glyph_cache: LruCache::new(NonZeroUsize::new(GLYPH_CACHE_CAPACITY).unwrap()),
            color_glyph_cache: LruCache::new(
                NonZeroUsize::new(COLOR_GLYPH_CACHE_CAPACITY).unwrap(),
            ),
        }
    }

    /// Get the shared font registry
    ///
    /// This can be used to share the registry with other components
    /// like text measurement.
    pub fn font_registry(&self) -> Arc<std::sync::Mutex<FontRegistry>> {
        self.font_registry.clone()
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

    /// Load font data into the registry (used by the rendering system)
    ///
    /// This adds fonts to the registry where they can be found by name
    /// during text rendering. Returns the number of font faces loaded.
    pub fn load_font_data_to_registry(&mut self, data: Vec<u8>) -> usize {
        let mut registry = self.font_registry.lock().unwrap();
        registry.load_font_data(data)
    }

    /// Get the glyph atlas (grayscale)
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    /// Get mutable atlas (for GPU upload checking)
    pub fn atlas_mut(&mut self) -> &mut GlyphAtlas {
        &mut self.atlas
    }

    /// Get the color glyph atlas (RGBA for emoji)
    pub fn color_atlas(&self) -> &ColorGlyphAtlas {
        &self.color_atlas
    }

    /// Get mutable color atlas
    pub fn color_atlas_mut(&mut self) -> &mut ColorGlyphAtlas {
        &mut self.color_atlas
    }

    /// Check if atlas needs GPU upload
    pub fn atlas_is_dirty(&self) -> bool {
        self.atlas.is_dirty()
    }

    /// Check if color atlas needs GPU upload
    pub fn color_atlas_is_dirty(&self) -> bool {
        self.color_atlas.is_dirty()
    }

    /// Mark atlas as clean after GPU upload
    pub fn mark_atlas_clean(&mut self) {
        self.atlas.mark_clean();
    }

    /// Mark color atlas as clean after GPU upload
    pub fn mark_color_atlas_clean(&mut self) {
        self.color_atlas.mark_clean();
    }

    /// Get atlas pixel data for GPU upload (grayscale)
    pub fn atlas_pixels(&self) -> &[u8] {
        self.atlas.pixels()
    }

    /// Get color atlas pixel data for GPU upload (RGBA)
    pub fn color_atlas_pixels(&self) -> &[u8] {
        self.color_atlas.pixels()
    }

    /// Get atlas dimensions
    pub fn atlas_dimensions(&self) -> (u32, u32) {
        self.atlas.dimensions()
    }

    /// Get color atlas dimensions
    pub fn color_atlas_dimensions(&self) -> (u32, u32) {
        self.color_atlas.dimensions()
    }

    /// Prepare text for rendering, rasterizing glyphs as needed
    pub fn prepare_text(
        &mut self,
        text: &str,
        font_size: f32,
        color: [f32; 4],
        options: &LayoutOptions,
    ) -> Result<PreparedText> {
        self.prepare_text_internal(
            text,
            font_size,
            color,
            options,
            None,
            GenericFont::System,
            400,
            false,
        )
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
        self.prepare_text_internal(
            text, font_size, color, options, font_name, generic, 400, false,
        )
    }

    /// Prepare text for rendering with a specific font family, weight, and style
    ///
    /// # Arguments
    /// * `text` - The text to render
    /// * `font_size` - Font size in pixels
    /// * `color` - Text color (RGBA, 0.0-1.0)
    /// * `options` - Layout options
    /// * `font_name` - Optional font name (e.g., "Fira Code", "Inter")
    /// * `generic` - Generic font fallback category
    /// * `weight` - Font weight (100-900, where 400 is normal, 700 is bold)
    /// * `italic` - Whether to use italic variant
    pub fn prepare_text_with_style(
        &mut self,
        text: &str,
        font_size: f32,
        color: [f32; 4],
        options: &LayoutOptions,
        font_name: Option<&str>,
        generic: GenericFont,
        weight: u16,
        italic: bool,
    ) -> Result<PreparedText> {
        self.prepare_text_internal(
            text, font_size, color, options, font_name, generic, weight, italic,
        )
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
        weight: u16,
        italic: bool,
    ) -> Result<PreparedText> {
        // Resolve the font to use
        let font = self.resolve_font_with_style(font_name, generic, weight, italic)?;
        let font_id = self.font_id_with_style(font_name, generic, weight, italic);

        // Get font metrics for the PreparedText result
        let (ascender, descender) = {
            let metrics = font.metrics();
            (
                metrics.ascender_px(font_size),
                metrics.descender_px(font_size),
            )
        };

        // Lazy-loaded fallback fonts: only load emoji/symbol fonts when actually needed
        // This saves ~180MB of memory when text doesn't contain emoji
        // Emoji font and symbol font are loaded separately - symbol font is small,
        // but emoji font (Apple Color Emoji) is ~180MB, so we only load it for actual emoji
        let mut emoji_font: Option<Arc<FontFace>> = None;
        let mut symbol_font: Option<Arc<FontFace>> = None;
        let mut emoji_font_id: u32 = 0;
        let mut symbol_font_id: u32 = 0;
        let mut emoji_font_loaded = false;
        let mut symbol_font_loaded = false;

        // Layout the text
        let layout = self.layout_engine.layout(text, &font, font_size, options);

        // Collect positioned glyphs for processing
        let positioned_glyphs: Vec<_> = layout.glyphs().cloned().collect();

        // Convert to GPU glyph instances
        let mut glyphs = Vec::with_capacity(positioned_glyphs.len());
        let atlas_dims = self.atlas.dimensions();
        let color_atlas_dims = self.color_atlas.dimensions();

        // Track glyph info along with whether it's a color glyph
        // (GlyphInfo, PositionedGlyph, is_color)
        struct RasterizedGlyphData {
            info: GlyphInfo,
            positioned: PositionedGlyph,
            is_color: bool,
        }

        let mut glyph_infos: Vec<Option<RasterizedGlyphData>> =
            Vec::with_capacity(positioned_glyphs.len());

        // Track advance correction when using fallback fonts
        // This accumulates the difference between what the primary font gave us
        // and what the fallback font's actual advance is
        let mut x_offset: f32 = 0.0;

        for (i, positioned) in positioned_glyphs.iter().enumerate() {
            if positioned.codepoint.is_whitespace() {
                glyph_infos.push(None);
                continue;
            }

            // Skip invisible combining characters
            // - Variation selectors (U+FE00-U+FE0F) modify the previous character's style
            // - ZWJ (U+200D) joins emoji into sequences
            // These are handled by the shaper but shouldn't render as visible glyphs
            if is_variation_selector(positioned.codepoint) || is_zwj(positioned.codepoint) {
                glyph_infos.push(None);
                continue;
            }

            // Check if this is an emoji or if the primary font doesn't have this glyph
            let is_emoji_char = is_emoji(positioned.codepoint);

            // For emoji characters, check if we've already processed this exact codepoint
            // at a previous position. This handles cases where HarfBuzz produces multiple
            // glyphs for a single emoji sequence (e.g., ☀️ = sun + variation selector).
            // The shaper may report both glyphs with the same codepoint due to cluster mapping.
            if is_emoji_char {
                // Check if the previous glyph was the same emoji codepoint
                // If so, this is likely a duplicate from cluster mapping and should be skipped
                if i > 0 {
                    let prev = &positioned_glyphs[i - 1];
                    if prev.codepoint == positioned.codepoint && is_emoji(prev.codepoint) {
                        // Skip this duplicate emoji glyph
                        glyph_infos.push(None);
                        continue;
                    }
                }
            }

            // Check if fallback is needed:
            // - Primary font doesn't have this glyph (glyph_id == 0 or has_glyph returns false)
            // - For emoji, always try emoji font to get color rendering (even if primary has glyph)
            let primary_has_glyph =
                positioned.glyph_id != 0 && font.has_glyph(positioned.codepoint);
            let needs_fallback = !primary_has_glyph || is_emoji_char;

            if needs_fallback {
                // Lazy load symbol font for non-emoji fallback (small, fast to load)
                if !symbol_font_loaded {
                    let mut registry = self.font_registry.lock().unwrap();
                    symbol_font = registry.load_generic(GenericFont::Symbol).ok();
                    drop(registry);
                    symbol_font_id = self.font_id(None, GenericFont::Symbol);
                    symbol_font_loaded = true;
                }

                // Only load emoji font (~180MB) when we actually encounter an emoji character
                if is_emoji_char && !emoji_font_loaded {
                    let mut registry = self.font_registry.lock().unwrap();
                    emoji_font = registry.load_generic(GenericFont::Emoji).ok();
                    drop(registry);
                    emoji_font_id = self.font_id(None, GenericFont::Emoji);
                    emoji_font_loaded = true;
                }

                // Build fallback font chain: try emoji first (for emoji), then symbol (for Unicode symbols)
                // For non-emoji characters, prefer symbol font to get text-colored glyphs
                let fallback_fonts: Vec<(&Arc<FontFace>, u32, bool)> = if is_emoji_char {
                    // Emoji: try emoji font first (color), then symbol (grayscale)
                    [
                        emoji_font.as_ref().map(|f| (f, emoji_font_id, true)),
                        symbol_font.as_ref().map(|f| (f, symbol_font_id, false)),
                    ]
                    .into_iter()
                    .flatten()
                    .collect()
                } else {
                    // Non-emoji: only use symbol font (don't load emoji font for non-emoji characters)
                    [symbol_font.as_ref().map(|f| (f, symbol_font_id, false))]
                        .into_iter()
                        .flatten()
                        .collect()
                };

                let mut found_fallback = false;
                for (fallback_font, fallback_font_id, use_color) in &fallback_fonts {
                    if let Some(fallback_glyph_id) = fallback_font.glyph_id(positioned.codepoint) {
                        if fallback_glyph_id != 0 {
                            // Shape just this character with the fallback font to get correct metrics
                            let shaper = TextShaper::new();
                            // Use stack-allocated buffer instead of heap String
                            let mut char_buf = [0u8; 4];
                            let char_str = positioned.codepoint.encode_utf8(&mut char_buf);
                            let shaped = shaper.shape(char_str, fallback_font, font_size);

                            if let Some(shaped_glyph) = shaped.glyphs.first() {
                                // Create a new positioned glyph with fallback font metrics
                                // Apply the accumulated x_offset from previous fallback corrections
                                let fallback_positioned = PositionedGlyph {
                                    glyph_id: shaped_glyph.glyph_id,
                                    codepoint: positioned.codepoint,
                                    x: positioned.x + x_offset,
                                    y: positioned.y,
                                };

                                // Use color rasterization for emoji font
                                let (glyph_info, is_color) = if *use_color && is_emoji_char {
                                    let info = self.rasterize_color_glyph_for_font(
                                        fallback_font,
                                        *fallback_font_id,
                                        shaped_glyph.glyph_id,
                                        font_size,
                                    )?;
                                    (info, true)
                                } else {
                                    let info = self.rasterize_glyph_for_font(
                                        fallback_font,
                                        *fallback_font_id,
                                        shaped_glyph.glyph_id,
                                        font_size,
                                    )?;
                                    (info, false)
                                };

                                // Calculate advance correction
                                // The fallback font's advance tells us how much space this glyph needs
                                let fallback_advance = glyph_info.advance as f32;

                                // Calculate what advance the primary font thought this character had
                                // by looking at the distance to the next glyph
                                let primary_advance = if i + 1 < positioned_glyphs.len() {
                                    positioned_glyphs[i + 1].x - positioned.x
                                } else {
                                    // Last character - use layout width
                                    (layout.width - positioned.x).max(0.0)
                                };

                                // Accumulate the difference
                                x_offset += fallback_advance - primary_advance;

                                glyph_infos.push(Some(RasterizedGlyphData {
                                    info: glyph_info,
                                    positioned: fallback_positioned,
                                    is_color,
                                }));
                                found_fallback = true;
                                break;
                            }
                        }
                    }
                }

                if found_fallback {
                    continue;
                }
            }

            // Use primary font (apply accumulated x_offset)
            let glyph_info =
                self.rasterize_glyph_for_font(&font, font_id, positioned.glyph_id, font_size)?;
            let mut adjusted_positioned = positioned.clone();
            adjusted_positioned.x += x_offset;
            glyph_infos.push(Some(RasterizedGlyphData {
                info: glyph_info,
                positioned: adjusted_positioned,
                is_color: false,
            }));
        }

        // Second pass: build glyph instances
        for glyph_data in &glyph_infos {
            let data = match glyph_data {
                Some(d) => d,
                None => continue,
            };

            // Skip glyphs with no bitmap (empty glyphs)
            if data.info.region.width == 0 || data.info.region.height == 0 {
                continue;
            }

            // Calculate screen position
            // positioned.x is the pen position from the shaper (includes advance)
            // bearing_x is the offset from pen position to the glyph's left edge
            let x = data.positioned.x + data.info.bearing_x as f32;
            let y = data.positioned.y - data.info.bearing_y as f32;
            let w = data.info.region.width as f32;
            let h = data.info.region.height as f32;

            // Get UV coordinates from the appropriate atlas
            let uv = if data.is_color {
                data.info
                    .region
                    .uv_bounds(color_atlas_dims.0, color_atlas_dims.1)
            } else {
                data.info.region.uv_bounds(atlas_dims.0, atlas_dims.1)
            };

            glyphs.push(GlyphInstance {
                bounds: [x, y, w, h],
                uv_bounds: uv,
                color,
                is_color: data.is_color,
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

            // Skip invisible combining characters
            if is_variation_selector(positioned.codepoint) || is_zwj(positioned.codepoint) {
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

        for (i, (positioned, glyph_info)) in
            positioned_glyphs.iter().zip(glyph_infos.iter()).enumerate()
        {
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
                is_color: false,
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
        &mut self,
        font_name: Option<&str>,
        generic: GenericFont,
    ) -> Result<Arc<FontFace>> {
        self.resolve_font_with_style(font_name, generic, 400, false)
    }

    /// Resolve font by name or generic category with specific weight and style
    /// Loads fonts on demand if not cached
    fn resolve_font_with_style(
        &mut self,
        font_name: Option<&str>,
        generic: GenericFont,
        weight: u16,
        italic: bool,
    ) -> Result<Arc<FontFace>> {
        let mut registry = self.font_registry.lock().unwrap();

        // First try cache lookup
        if let Some(font) = registry.get_for_render_with_style(font_name, generic, weight, italic) {
            return Ok(font);
        }

        // Try loading the font with style on demand
        if let Some(name) = font_name {
            if let Ok(font) = registry.load_font_with_style(name, weight, italic) {
                return Ok(font);
            }
        }

        // Try loading generic font with style
        if let Ok(font) = registry.load_generic_with_style(generic, weight, italic) {
            return Ok(font);
        }

        // If styled font not found, fall back to normal style
        if weight != 400 || italic {
            if let Some(font) = registry.get_for_render_with_style(font_name, generic, 400, false) {
                return Ok(font);
            }
            // Try loading normal style
            if let Ok(font) = registry.load_generic_with_style(generic, 400, false) {
                return Ok(font);
            }
        }

        // Ultimate fallback to SansSerif normal
        if let Some(font) = registry.get_cached_generic(GenericFont::SansSerif) {
            return Ok(font);
        }
        if let Ok(font) = registry.load_generic(GenericFont::SansSerif) {
            return Ok(font);
        }

        Err(TextError::FontLoadError("No fonts available".to_string()))
    }

    /// Preload fonts that your app uses (call at startup)
    pub fn preload_fonts(&mut self, names: &[&str]) {
        let mut registry = self.font_registry.lock().unwrap();
        registry.preload_fonts(names);
    }

    /// Preload fonts with specific weights and styles
    pub fn preload_fonts_with_styles(&mut self, specs: &[(&str, u16, bool)]) {
        let mut registry = self.font_registry.lock().unwrap();
        for (name, weight, italic) in specs {
            let _ = registry.load_font_with_style(name, *weight, *italic);
        }
    }

    /// Preload generic font variants (weights and italic)
    pub fn preload_generic_styles(&mut self, generic: GenericFont, weights: &[u16], italic: bool) {
        let mut registry = self.font_registry.lock().unwrap();
        for weight in weights {
            let _ = registry.load_generic_with_style(generic, *weight, italic);
        }
    }

    /// Generate a unique font ID for cache keys
    fn font_id(&self, font_name: Option<&str>, generic: GenericFont) -> u32 {
        self.font_id_with_style(font_name, generic, 400, false)
    }

    /// Generate a unique font ID for cache keys with style
    fn font_id_with_style(
        &self,
        font_name: Option<&str>,
        generic: GenericFont,
        weight: u16,
        italic: bool,
    ) -> u32 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        font_name.hash(&mut hasher);
        generic.hash(&mut hasher);
        weight.hash(&mut hasher);
        italic.hash(&mut hasher);
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

        // Check cache first (LruCache::get promotes to most-recently-used)
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
            // LruCache::put evicts oldest entry if at capacity
            self.glyph_cache.put(cache_key, info);
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

        self.glyph_cache.put(cache_key, info);
        Ok(info)
    }

    /// Rasterize a color glyph (emoji) for a specific font
    fn rasterize_color_glyph_for_font(
        &mut self,
        font: &FontFace,
        font_id: u32,
        glyph_id: u16,
        font_size: f32,
    ) -> Result<GlyphInfo> {
        // Quantize font size for cache key (0.5px granularity)
        let size_key = (font_size * 2.0).round() as u16;
        let cache_key = (font_id, glyph_id, size_key);

        // Check color cache first (LruCache::get promotes to most-recently-used)
        if let Some(info) = self.color_glyph_cache.get(&cache_key) {
            return Ok(*info);
        }

        // Rasterize the glyph as color (RGBA)
        let rasterized = self.rasterizer.rasterize_color(font, glyph_id, font_size)?;

        // Handle empty glyphs
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
            // LruCache::put evicts oldest entry if at capacity
            self.color_glyph_cache.put(cache_key, info);
            return Ok(info);
        }

        // Insert into color atlas
        let info = self.color_atlas.insert_glyph(
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

        self.color_glyph_cache.put(cache_key, info);
        Ok(info)
    }

    /// Legacy method for backward compatibility - uses system font from registry
    #[allow(dead_code)]
    fn rasterize_glyph_if_needed(&mut self, glyph_id: u16, font_size: f32) -> Result<GlyphInfo> {
        let font = {
            let mut registry = self.font_registry.lock().unwrap();
            registry.load_generic(GenericFont::SansSerif)?
        };
        self.rasterize_glyph_for_font(&font, 0, glyph_id, font_size)
    }

    /// Clear the glyph cache and atlas
    pub fn clear(&mut self) {
        self.atlas.clear();
        self.color_atlas.clear();
        self.glyph_cache.clear();
        self.color_glyph_cache.clear();
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}
