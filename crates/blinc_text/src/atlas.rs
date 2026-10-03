//! Glyph atlas management
//!
//! Manages a texture atlas for caching rendered glyphs. Uses a skyline/shelf
//! packing algorithm for efficient space utilization.
//!
//! Provides two atlas types:
//! - `GlyphAtlas`: Grayscale atlas for regular text glyphs
//! - `ColorGlyphAtlas`: RGBA atlas for color emoji

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
    /// Font ID (hash of font name/family)
    font_id: u32,
    /// Glyph ID in the font
    glyph_id: u16,
    /// Font size (quantized to avoid too many entries)
    size_key: u16,
}

impl GlyphKey {
    fn new(font_id: u32, glyph_id: u16, font_size: f32) -> Self {
        // Quantize font size to reduce cache entries (0.5px granularity)
        let size_key = (font_size * 2.0).round() as u16;
        Self {
            font_id,
            glyph_id,
            size_key,
        }
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

/// The union of the atlas regions written since the last `mark_clean`.
///
/// One source of truth for dirtiness: `None` is clean, so the boolean
/// and the rect cannot disagree. A caller uploads just this rect
/// instead of the whole atlas.
#[derive(Debug, Clone, Copy, Default)]
struct DirtyRegion {
    rect: Option<(u32, u32, u32, u32)>,
}

impl DirtyRegion {
    /// Nothing to upload.
    fn clean() -> Self {
        Self { rect: None }
    }

    /// The whole atlas, for a resize or a clear.
    fn all(width: u32, height: u32) -> Self {
        Self {
            rect: Some((0, 0, width, height)),
        }
    }

    /// Grow the region to cover `(x, y, w, h)` as well.
    fn add(&mut self, x: u32, y: u32, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.rect = Some(match self.rect {
            None => (x, y, w, h),
            Some((cx, cy, cw, ch)) => {
                let x0 = cx.min(x);
                let y0 = cy.min(y);
                let x1 = (cx + cw).max(x + w);
                let y1 = (cy + ch).max(y + h);
                (x0, y0, x1 - x0, y1 - y0)
            }
        });
    }

    fn rect(&self) -> Option<(u32, u32, u32, u32)> {
        self.rect
    }

    fn is_dirty(&self) -> bool {
        self.rect.is_some()
    }
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
    /// Which atlas region changed since the last upload
    dirty: DirtyRegion,
}

impl GlyphAtlas {
    /// Maximum atlas dimension (4096×4096 = 16 MB for R8)
    const MAX_SIZE: u32 = 4096;

    /// Create a new glyph atlas
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
            glyphs: FxHashMap::default(),
            shelves: Vec::new(),
            padding: 2, // 2 pixel padding between glyphs
            dirty: DirtyRegion::all(width, height),
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
        self.dirty.is_dirty()
    }

    /// The region written since the last `mark_clean`, as
    /// `(x, y, width, height)` in atlas pixels. `None` when clean.
    pub fn dirty_rect(&self) -> Option<(u32, u32, u32, u32)> {
        self.dirty.rect()
    }

    /// Mark atlas as clean (after GPU upload)
    pub fn mark_clean(&mut self) {
        self.dirty = DirtyRegion::clean();
    }

    /// Look up a cached glyph
    pub fn get_glyph(&self, font_id: u32, glyph_id: u16, font_size: f32) -> Option<&GlyphInfo> {
        let key = GlyphKey::new(font_id, glyph_id, font_size);
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
            if shelf.height >= padded_height
                && shelf.x + padded_width <= self.width
                && shelf.y < best_y
            {
                best_y = shelf.y;
                best_shelf = Some(i);
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
    #[allow(clippy::too_many_arguments)]
    pub fn insert_glyph(
        &mut self,
        font_id: u32,
        glyph_id: u16,
        font_size: f32,
        width: u32,
        height: u32,
        bearing_x: i16,
        bearing_y: i16,
        advance: u16,
        bitmap: &[u8],
    ) -> Result<GlyphInfo> {
        let key = GlyphKey::new(font_id, glyph_id, font_size);

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
        // The padding the allocator reserves is never written, so it
        // stays zero and does not need uploading.
        self.dirty.add(region.x, region.y, width, height);

        Ok(info)
    }

    /// Double the atlas dimensions and repack existing pixel data.
    ///
    /// Old pixel data is copied into the top-left quadrant of the new buffer.
    /// Existing `AtlasRegion` pixel coords remain valid because `uv_bounds()`
    /// recomputes UVs dynamically from the (now larger) atlas dimensions.
    /// Returns `true` if growth succeeded, `false` if already at max size.
    pub fn grow(&mut self) -> bool {
        let new_width = (self.width * 2).min(Self::MAX_SIZE);
        let new_height = (self.height * 2).min(Self::MAX_SIZE);

        if new_width == self.width && new_height == self.height {
            return false; // Already at max size
        }

        // Copy old pixel data row-by-row into top-left of new buffer
        let mut new_pixels = vec![0u8; (new_width * new_height) as usize];
        for y in 0..self.height {
            let src_start = (y * self.width) as usize;
            let src_end = src_start + self.width as usize;
            let dst_start = (y * new_width) as usize;
            let dst_end = dst_start + self.width as usize;
            new_pixels[dst_start..dst_end].copy_from_slice(&self.pixels[src_start..src_end]);
        }

        self.pixels = new_pixels;
        self.width = new_width;
        self.height = new_height;
        self.dirty = DirtyRegion::all(new_width, new_height);
        true
    }

    /// Clear all cached glyphs
    pub fn clear(&mut self) {
        self.glyphs.clear();
        self.shelves.clear();
        self.pixels.fill(0);
        self.dirty = DirtyRegion::all(self.width, self.height);
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
        // 1024x1024 atlas (1 MB for R8) — large enough for most UIs without growing.
        // grow() doubles dimensions (up to 4096) if more space is needed.
        Self::new(1024, 1024)
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
            .field("dirty_rect", &self.dirty.rect())
            .finish()
    }
}

/// Color glyph atlas for RGBA emoji
///
/// Similar to GlyphAtlas but stores RGBA pixel data (4 bytes per pixel)
/// for color emoji and other color glyphs.
///
/// The CPU shadow buffer is **lazily allocated** on first
/// `insert_glyph`. An app that never renders a color emoji pays
/// nothing — this used to allocate 512×512×4 = 1 MB at startup
/// regardless. The companion GPU texture in `blinc_gpu::text` is also
/// lazy; both stay `None` together until the first insert.
pub struct ColorGlyphAtlas {
    /// Atlas width in pixels
    width: u32,
    /// Atlas height in pixels
    height: u32,
    /// Pixel data (RGBA, 4 bytes per pixel). `None` until first insert.
    pixels: Option<Vec<u8>>,
    /// Cached glyph information
    glyphs: FxHashMap<GlyphKey, GlyphInfo>,
    /// Shelves for skyline packing
    shelves: Vec<Shelf>,
    /// Padding between glyphs
    padding: u32,
    /// Which atlas region changed since the last upload
    dirty: DirtyRegion,
}

impl ColorGlyphAtlas {
    /// Maximum atlas dimension (4096×4096 = 64 MB for RGBA)
    const MAX_SIZE: u32 = 4096;

    /// Create a new color glyph atlas. The pixel buffer is not
    /// allocated until first `insert_glyph` — see struct doc.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: None,
            glyphs: FxHashMap::default(),
            shelves: Vec::new(),
            padding: 2,
            // Stays clean until something is actually inserted. Was
            // previously dirty because `new` allocated a fresh (empty)
            // buffer; with lazy alloc there is nothing to upload.
            dirty: DirtyRegion::clean(),
        }
    }

    /// Allocate the CPU pixel buffer if not yet allocated. Idempotent.
    fn ensure_alloc(&mut self) {
        if self.pixels.is_none() {
            self.pixels = Some(vec![0u8; (self.width * self.height * 4) as usize]);
        }
    }

    /// Get atlas dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get raw pixel data (RGBA format). Returns an empty slice when
    /// the atlas hasn't been touched (no color glyph ever inserted).
    pub fn pixels(&self) -> &[u8] {
        self.pixels.as_deref().unwrap_or(&[])
    }

    /// Check if atlas has been modified
    pub fn is_dirty(&self) -> bool {
        self.dirty.is_dirty()
    }

    /// The region written since the last `mark_clean`, as
    /// `(x, y, width, height)` in atlas pixels. `None` when clean.
    pub fn dirty_rect(&self) -> Option<(u32, u32, u32, u32)> {
        self.dirty.rect()
    }

    /// Mark atlas as clean (after GPU upload)
    pub fn mark_clean(&mut self) {
        self.dirty = DirtyRegion::clean();
    }

    /// Look up a cached glyph
    pub fn get_glyph(&self, font_id: u32, glyph_id: u16, font_size: f32) -> Option<&GlyphInfo> {
        let key = GlyphKey::new(font_id, glyph_id, font_size);
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
            if shelf.height >= padded_height
                && shelf.x + padded_width <= self.width
                && shelf.y < best_y
            {
                best_y = shelf.y;
                best_shelf = Some(i);
            }
        }

        if let Some(shelf_idx) = best_shelf {
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

    /// Insert a rasterized color glyph (RGBA) into the atlas
    #[allow(clippy::too_many_arguments)]
    pub fn insert_glyph(
        &mut self,
        font_id: u32,
        glyph_id: u16,
        font_size: f32,
        width: u32,
        height: u32,
        bearing_x: i16,
        bearing_y: i16,
        advance: u16,
        bitmap: &[u8],
    ) -> Result<GlyphInfo> {
        let key = GlyphKey::new(font_id, glyph_id, font_size);

        // Check if already cached
        if let Some(info) = self.glyphs.get(&key) {
            return Ok(*info);
        }

        // Lazy: allocate the 1 MB pixel buffer only on first insert.
        self.ensure_alloc();

        // Allocate region
        let region = self.allocate(width, height)?;

        // Copy RGBA bitmap to atlas (4 bytes per pixel)
        let pixels = self.pixels.as_mut().expect("ensure_alloc above");
        for y in 0..height {
            let src_offset = (y * width * 4) as usize;
            let dst_offset = ((region.y + y) * self.width * 4 + region.x * 4) as usize;
            let row_bytes = (width * 4) as usize;

            if src_offset + row_bytes <= bitmap.len() && dst_offset + row_bytes <= pixels.len() {
                pixels[dst_offset..dst_offset + row_bytes]
                    .copy_from_slice(&bitmap[src_offset..src_offset + row_bytes]);
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
        // The padding the allocator reserves is never written, so it
        // stays zero and does not need uploading.
        self.dirty.add(region.x, region.y, width, height);

        Ok(info)
    }

    /// Double the atlas dimensions and repack existing pixel data (RGBA).
    ///
    /// Same approach as `GlyphAtlas::grow()` but with 4 bytes per pixel.
    /// Returns `true` if growth succeeded, `false` if already at max
    /// size or if the atlas is still unallocated (`new` was called but
    /// no `insert_glyph` ever ran — nothing to grow).
    pub fn grow(&mut self) -> bool {
        let new_width = (self.width * 2).min(Self::MAX_SIZE);
        let new_height = (self.height * 2).min(Self::MAX_SIZE);

        if new_width == self.width && new_height == self.height {
            return false;
        }

        let Some(old_pixels) = self.pixels.as_ref() else {
            return false;
        };

        let mut new_pixels = vec![0u8; (new_width * new_height * 4) as usize];
        for y in 0..self.height {
            let src_start = (y * self.width * 4) as usize;
            let row_bytes = (self.width * 4) as usize;
            let dst_start = (y * new_width * 4) as usize;
            new_pixels[dst_start..dst_start + row_bytes]
                .copy_from_slice(&old_pixels[src_start..src_start + row_bytes]);
        }

        self.pixels = Some(new_pixels);
        self.width = new_width;
        self.height = new_height;
        self.dirty = DirtyRegion::all(new_width, new_height);
        true
    }

    /// Clear all cached glyphs. Pixel buffer is zeroed only if it was
    /// ever allocated.
    pub fn clear(&mut self) {
        self.glyphs.clear();
        self.shelves.clear();
        // Only dirty if there is a buffer to upload. Unallocated means
        // nothing was ever written, so a clear changes nothing.
        if let Some(pixels) = self.pixels.as_mut() {
            pixels.fill(0);
            self.dirty = DirtyRegion::all(self.width, self.height);
        }
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

impl Default for ColorGlyphAtlas {
    fn default() -> Self {
        // 512x512 atlas (1 MB for RGBA) — sufficient for typical emoji usage.
        // grow() doubles dimensions (up to 4096) if more space is needed.
        Self::new(512, 512)
    }
}

impl std::fmt::Debug for ColorGlyphAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColorGlyphAtlas")
            .field("dimensions", &(self.width, self.height))
            .field("glyph_count", &self.glyphs.len())
            .field(
                "utilization",
                &format!("{:.1}%", self.utilization() * 100.0),
            )
            .field("dirty_rect", &self.dirty.rect())
            .finish()
    }
}

#[cfg(test)]
mod dirty_rect_tests {
    use super::*;

    /// A 4x4 glyph of solid coverage.
    fn bitmap(w: u32, h: u32) -> Vec<u8> {
        vec![0xff; (w * h) as usize]
    }

    fn insert(atlas: &mut GlyphAtlas, glyph_id: u16, w: u32, h: u32) -> GlyphInfo {
        atlas
            .insert_glyph(0, glyph_id, 16.0, w, h, 0, 0, w as u16, &bitmap(w, h))
            .expect("fits")
    }

    #[test]
    fn a_clean_atlas_reports_no_rect() {
        let mut atlas = GlyphAtlas::new(128, 128);
        atlas.mark_clean();
        assert_eq!(atlas.dirty_rect(), None);
        assert!(!atlas.is_dirty());
    }

    /// The rect covers the glyph that was written, not the whole atlas.
    /// That is the entire point: uploading the whole atlas per glyph is
    /// what this replaces.
    #[test]
    fn one_glyph_dirties_only_its_own_region() {
        let mut atlas = GlyphAtlas::new(128, 128);
        atlas.mark_clean();
        let info = insert(&mut atlas, 1, 8, 12);

        let (x, y, w, h) = atlas.dirty_rect().expect("dirty after insert");
        assert_eq!((x, y), (info.region.x, info.region.y));
        assert_eq!((w, h), (8, 12));
        assert!(w < 128 && h < 128, "should not be the whole atlas");
    }

    /// Two glyphs give the bounding box of both, so a single upload
    /// covers them.
    #[test]
    fn two_glyphs_union_into_one_rect() {
        let mut atlas = GlyphAtlas::new(128, 128);
        atlas.mark_clean();
        let a = insert(&mut atlas, 1, 8, 8);
        let b = insert(&mut atlas, 2, 8, 8);

        let (x, y, w, h) = atlas.dirty_rect().expect("dirty");
        let x0 = a.region.x.min(b.region.x);
        let y0 = a.region.y.min(b.region.y);
        let x1 = (a.region.x + 8).max(b.region.x + 8);
        let y1 = (a.region.y + 8).max(b.region.y + 8);
        assert_eq!((x, y, w, h), (x0, y0, x1 - x0, y1 - y0));
    }

    /// Re-requesting a cached glyph writes nothing, so it must not
    /// re-dirty the atlas.
    #[test]
    fn a_cache_hit_does_not_dirty() {
        let mut atlas = GlyphAtlas::new(128, 128);
        insert(&mut atlas, 1, 8, 8);
        atlas.mark_clean();

        insert(&mut atlas, 1, 8, 8);
        assert_eq!(atlas.dirty_rect(), None, "cache hit wrote nothing");
    }

    /// Growing moves every pixel into a new, larger buffer, so the whole
    /// thing has to go up.
    #[test]
    fn grow_dirties_the_whole_atlas() {
        let mut atlas = GlyphAtlas::new(128, 128);
        insert(&mut atlas, 1, 8, 8);
        atlas.mark_clean();

        assert!(atlas.grow());
        let (w, h) = atlas.dimensions();
        assert_eq!(atlas.dirty_rect(), Some((0, 0, w, h)));
    }

    #[test]
    fn clear_dirties_the_whole_atlas() {
        let mut atlas = GlyphAtlas::new(128, 128);
        atlas.mark_clean();

        atlas.clear();
        assert_eq!(atlas.dirty_rect(), Some((0, 0, 128, 128)));
    }

    /// The colour atlas allocates lazily, so it starts clean and the
    /// same rules apply once something lands in it.
    #[test]
    fn the_color_atlas_tracks_its_own_region() {
        let mut atlas = ColorGlyphAtlas::new(128, 128);
        assert_eq!(
            atlas.dirty_rect(),
            None,
            "lazy alloc means nothing to upload"
        );

        let rgba = vec![0xff; (6 * 6 * 4) as usize];
        let info = atlas
            .insert_glyph(0, 1, 16.0, 6, 6, 0, 0, 6, &rgba)
            .expect("fits");

        assert_eq!(
            atlas.dirty_rect(),
            Some((info.region.x, info.region.y, 6, 6))
        );
        atlas.mark_clean();
        assert_eq!(atlas.dirty_rect(), None);
    }

    /// Clearing an atlas that never allocated must not ask for an
    /// upload: `pixels()` is empty, and a non-empty extent from an empty
    /// slice is a wgpu validation error.
    #[test]
    fn clearing_an_unallocated_color_atlas_stays_clean() {
        let mut atlas = ColorGlyphAtlas::new(128, 128);
        atlas.clear();
        assert_eq!(atlas.dirty_rect(), None);
        assert!(atlas.pixels().is_empty());
    }

    /// Once allocated, a clear does need the whole atlas uploaded.
    #[test]
    fn clearing_an_allocated_color_atlas_dirties_it() {
        let mut atlas = ColorGlyphAtlas::new(128, 128);
        let rgba = vec![0xff; (4 * 4 * 4) as usize];
        atlas
            .insert_glyph(0, 1, 16.0, 4, 4, 0, 0, 4, &rgba)
            .expect("fits");
        atlas.mark_clean();

        atlas.clear();
        assert_eq!(atlas.dirty_rect(), Some((0, 0, 128, 128)));
    }

    /// is_dirty and dirty_rect read the same field, so they cannot
    /// disagree about whether an upload is needed.
    #[test]
    fn is_dirty_agrees_with_dirty_rect() {
        let mut atlas = GlyphAtlas::new(128, 128);
        assert_eq!(atlas.is_dirty(), atlas.dirty_rect().is_some());
        atlas.mark_clean();
        assert_eq!(atlas.is_dirty(), atlas.dirty_rect().is_some());
        insert(&mut atlas, 1, 8, 8);
        assert_eq!(atlas.is_dirty(), atlas.dirty_rect().is_some());
    }
}
