//! Glyph atlas management
//!
//! Manages a texture atlas for caching rendered glyphs. Uses a skyline/shelf
//! packing algorithm for efficient space utilization.

use crate::{Result, TextError};
use rustc_hash::FxHashMap;

/// Region in the atlas texture
#[derive(Debug, Clone, Copy)]
pub struct AtlasRegion {
    /// X position in atlas (pixels)
    pub x: u32,
    /// Y position in atlas (pixels)
    pub y: u32,
    /// Width in atlas (pixels)
    pub width: u32,
    /// Height in atlas (pixels)
    pub height: u32,
}

impl AtlasRegion {
    /// Get UV coordinates for this region given atlas dimensions
    pub fn uv_bounds(&self, atlas_width: u32, atlas_height: u32) -> [f32; 4] {
        let u_min = self.x as f32 / atlas_width as f32;
        let v_min = self.y as f32 / atlas_height as f32;
        let u_max = (self.x + self.width) as f32 / atlas_width as f32;
        let v_max = (self.y + self.height) as f32 / atlas_height as f32;
        [u_min, v_min, u_max, v_max]
    }
}

/// Information about a cached glyph
#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    /// Region in the atlas texture
    pub region: AtlasRegion,
    /// Horizontal bearing (offset from origin to left edge)
    pub bearing_x: i16,
    /// Vertical bearing (offset from baseline to top edge)
    pub bearing_y: i16,
    /// Horizontal advance to next glyph
    pub advance: u16,
    /// Font size this glyph was rasterized at
    pub font_size: f32,
}

/// Key for glyph cache lookup
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    /// Glyph ID in the font
    glyph_id: u16,
    /// Font size (quantized to avoid too many entries)
    size_key: u16,
}

impl GlyphKey {
    fn new(glyph_id: u16, font_size: f32) -> Self {
        // Quantize font size to reduce cache entries (0.5px granularity)
        let size_key = (font_size * 2.0).round() as u16;
        Self { glyph_id, size_key }
    }
}

/// A shelf in the skyline packing algorithm
#[derive(Debug)]
struct Shelf {
    /// Y position of this shelf
    y: u32,
    /// Height of this shelf
    height: u32,
    /// Current X position (next free space)
    x: u32,
}

/// Glyph atlas for caching rendered glyphs
pub struct GlyphAtlas {
    /// Atlas width in pixels
    width: u32,
    /// Atlas height in pixels
    height: u32,
    /// Pixel data (single channel, 8-bit grayscale or SDF values)
    pixels: Vec<u8>,
    /// Cached glyph information
    glyphs: FxHashMap<GlyphKey, GlyphInfo>,
    /// Shelves for skyline packing
    shelves: Vec<Shelf>,
    /// Padding between glyphs
    padding: u32,
    /// Whether atlas data has been modified since last upload
    dirty: bool,
}

impl GlyphAtlas {
    /// Create a new glyph atlas
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
            glyphs: FxHashMap::default(),
            shelves: Vec::new(),
            padding: 2, // 2 pixel padding between glyphs
            dirty: true,
        }
    }

    /// Get atlas dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get raw pixel data
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Check if atlas has been modified
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark atlas as clean (after GPU upload)
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Look up a cached glyph
    pub fn get_glyph(&self, glyph_id: u16, font_size: f32) -> Option<&GlyphInfo> {
        let key = GlyphKey::new(glyph_id, font_size);
        self.glyphs.get(&key)
    }

    /// Allocate space for a glyph using skyline packing
    fn allocate(&mut self, width: u32, height: u32) -> Result<AtlasRegion> {
        let padded_width = width + self.padding;
        let padded_height = height + self.padding;

        // Find best shelf (smallest height that fits)
        let mut best_shelf = None;
        let mut best_y = u32::MAX;

        for (i, shelf) in self.shelves.iter().enumerate() {
            // Check if glyph fits in this shelf
            if shelf.height >= padded_height && shelf.x + padded_width <= self.width {
                if shelf.y < best_y {
                    best_y = shelf.y;
                    best_shelf = Some(i);
                }
            }
        }

        if let Some(shelf_idx) = best_shelf {
            // Use existing shelf
            let shelf = &mut self.shelves[shelf_idx];
            let region = AtlasRegion {
                x: shelf.x,
                y: shelf.y,
                width,
                height,
            };
            shelf.x += padded_width;
            return Ok(region);
        }

        // Create new shelf
        let new_y = self.shelves.last().map(|s| s.y + s.height).unwrap_or(0);

        if new_y + padded_height > self.height {
            return Err(TextError::AtlasFull);
        }

        let region = AtlasRegion {
            x: 0,
            y: new_y,
            width,
            height,
        };

        self.shelves.push(Shelf {
            y: new_y,
            height: padded_height,
            x: padded_width,
        });

        Ok(region)
    }

    /// Insert a rasterized glyph into the atlas
    pub fn insert_glyph(
        &mut self,
        glyph_id: u16,
        font_size: f32,
        width: u32,
        height: u32,
        bearing_x: i16,
        bearing_y: i16,
        advance: u16,
        bitmap: &[u8],
    ) -> Result<GlyphInfo> {
        let key = GlyphKey::new(glyph_id, font_size);

        // Check if already cached
        if let Some(info) = self.glyphs.get(&key) {
            return Ok(*info);
        }

        // Allocate region
        let region = self.allocate(width, height)?;

        // Copy bitmap to atlas
        for y in 0..height {
            let src_offset = (y * width) as usize;
            let dst_offset = ((region.y + y) * self.width + region.x) as usize;
            let row_end = src_offset + width as usize;

            if row_end <= bitmap.len() && dst_offset + width as usize <= self.pixels.len() {
                self.pixels[dst_offset..dst_offset + width as usize]
                    .copy_from_slice(&bitmap[src_offset..row_end]);
            }
        }

        let info = GlyphInfo {
            region,
            bearing_x,
            bearing_y,
            advance,
            font_size,
        };

        self.glyphs.insert(key, info);
        self.dirty = true;

        Ok(info)
    }

    /// Clear all cached glyphs
    pub fn clear(&mut self) {
        self.glyphs.clear();
        self.shelves.clear();
        self.pixels.fill(0);
        self.dirty = true;
    }

    /// Get number of cached glyphs
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// Calculate atlas utilization (0.0 to 1.0)
    pub fn utilization(&self) -> f32 {
        let used_height = self.shelves.last().map(|s| s.y + s.height).unwrap_or(0);
        used_height as f32 / self.height as f32
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        // Default to 512x512 atlas (256 KB instead of 1 MB)
        // This is sufficient for most UI text; can be resized if needed
        Self::new(512, 512)
    }
}

impl std::fmt::Debug for GlyphAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlyphAtlas")
            .field("dimensions", &(self.width, self.height))
            .field("glyph_count", &self.glyphs.len())
            .field(
                "utilization",
                &format!("{:.1}%", self.utilization() * 100.0),
            )
            .field("dirty", &self.dirty)
            .finish()
    }
}
