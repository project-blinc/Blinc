//! SVG texture atlas for batched GPU rendering
//!
//! Packs all rasterized SVG icons into a single shared RGBA texture using
//! shelf-packing (skyline algorithm). Eliminates per-icon GPU textures and
//! enables single-draw-call rendering for all SVG instances in a frame.

use std::collections::{HashMap, HashSet};

/// Region allocated in the SVG atlas
#[derive(Debug, Clone, Copy)]
pub struct SvgAtlasRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl SvgAtlasRegion {
    /// Returns [u_min, v_min, u_max, v_max] matching GpuImageInstance.src_uv format
    pub fn uv_bounds(&self, atlas_w: u32, atlas_h: u32) -> [f32; 4] {
        let u_min = self.x as f32 / atlas_w as f32;
        let v_min = self.y as f32 / atlas_h as f32;
        let u_max = (self.x + self.width) as f32 / atlas_w as f32;
        let v_max = (self.y + self.height) as f32 / atlas_h as f32;
        [u_min, v_min, u_max, v_max]
    }
}

/// A shelf in the skyline packing algorithm
#[derive(Debug)]
struct Shelf {
    y: u32,
    height: u32,
    x: u32,
}

const INITIAL_SIZE: u32 = 1024;
const MAX_SIZE: u32 = 4096;
const PADDING: u32 = 2;

/// SVG texture atlas — packs rasterized SVGs into a single GPU texture.
///
/// The texture and CPU shadow buffer are **lazily allocated** on first
/// `insert()`. An app with no `svg()` elements pays nothing — this used
/// to allocate a 1024×1024 RGBA texture (4 MB GPU + 4 MB CPU shadow)
/// at startup whether the atlas was ever used or not.
pub struct SvgAtlas {
    width: u32,
    height: u32,
    /// Lazily allocated. `None` until first `insert()` call.
    pixels: Option<Vec<u8>>,
    shelves: Vec<Shelf>,
    entries: HashMap<u64, SvgAtlasRegion>,
    /// Lazily allocated. `None` until first `insert()` call.
    texture: Option<wgpu::Texture>,
    /// Lazily allocated. `None` until first `insert()` call.
    view: Option<wgpu::TextureView>,
    dirty: bool,
    /// Cache keys accessed since the last `end_frame()` call.
    /// Used by the mark-and-sweep eviction: when the atlas is full,
    /// `evict_unused()` clears entries that weren't accessed this
    /// frame and repacks the atlas from scratch. Without this,
    /// animated SVGs (each frame producing unique cache keys) fill
    /// the atlas permanently and all subsequent insertions fail.
    used_this_frame: HashSet<u64>,
}

impl SvgAtlas {
    pub fn new(_device: &wgpu::Device) -> Self {
        // Don't allocate the texture / shadow buffer yet. `ensure_alloc`
        // creates them on the first `insert()` call. For apps that
        // never render an SVG (a substantial fraction of the demos and
        // most production UIs), this saves ~8 MB of resident memory.
        Self {
            width: INITIAL_SIZE,
            height: INITIAL_SIZE,
            pixels: None,
            shelves: Vec::new(),
            entries: HashMap::new(),
            texture: None,
            view: None,
            dirty: false,
            used_this_frame: HashSet::new(),
        }
    }

    /// Allocate the texture + CPU shadow buffer if they haven't been
    /// allocated yet. Idempotent.
    fn ensure_alloc(&mut self, device: &wgpu::Device) {
        if self.texture.is_some() {
            return;
        }
        let (texture, view) = create_atlas_texture(device, self.width, self.height);
        self.pixels = Some(vec![0u8; (self.width * self.height * 4) as usize]);
        self.texture = Some(texture);
        self.view = Some(view);
    }

    /// Returns the GPU texture view for binding. Returns `None` when
    /// the atlas hasn't been allocated yet (no SVG rendered). Callers
    /// already gate on `instances.is_empty()` before binding, so this
    /// stays in sync with whether anything's been inserted.
    pub fn view(&self) -> Option<&wgpu::TextureView> {
        self.view.as_ref()
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Look up an existing entry by cache key
    pub fn get(&self, cache_key: u64) -> Option<&SvgAtlasRegion> {
        self.entries.get(&cache_key)
    }

    /// Mark a cache key as "used this frame" for eviction tracking.
    /// Call this for every SVG that actually gets rendered.
    pub fn mark_used(&mut self, cache_key: u64) {
        self.used_this_frame.insert(cache_key);
    }

    /// Copy one raster from the CPU shadow without retaining atlas UV coordinates.
    pub(crate) fn copy_pixels(&self, cache_key: u64) -> Option<(Vec<u8>, u32, u32)> {
        let region = self.entries.get(&cache_key)?;
        let source = self.pixels.as_ref()?;
        let stride = region.width as usize * 4;
        let mut pixels = Vec::with_capacity(stride * region.height as usize);
        for y in region.y..region.y + region.height {
            let start = ((y * self.width + region.x) * 4) as usize;
            pixels.extend_from_slice(&source[start..start + stride]);
        }
        Some((pixels, region.width, region.height))
    }

    /// Allocate space, write pixels, and insert a cache entry. Returns the region.
    ///
    /// Returns `None` if the atlas is full at the maximum size — callers
    /// should either fall back to a per-icon texture or simply skip the
    /// element for this frame. We deliberately do *not* `clear()` and
    /// retry on overflow at MAX_SIZE: that turns the atlas into a
    /// thrashing cache where every overflowing frame wipes valid entries
    /// and re-rasterizes them on the next frame, churning the GPU and
    /// keeping the texture pinned at the maximum 4096×4096 footprint
    /// (~67 MB CPU + ~67 MB GPU) forever. If repeated overflow happens,
    /// the warning surfaces it instead of silently degrading.
    pub fn insert(
        &mut self,
        cache_key: u64,
        width: u32,
        height: u32,
        rgba_pixels: &[u8],
        device: &wgpu::Device,
    ) -> Option<SvgAtlasRegion> {
        // First-time allocation: lazily create the texture + CPU shadow
        // buffer the first time anything is inserted. Apps that never
        // render an SVG never pay the ~8 MB cost.
        self.ensure_alloc(device);

        // Try to allocate — grow if needed, but never repack mid-frame.
        // Eviction runs in `begin_frame()` at the top of the render
        // pass so UV coordinates stay stable for the entire loop.
        let region = match self.allocate(width, height) {
            Some(r) => r,
            None => {
                if self.grow(device) {
                    self.allocate(width, height)?
                } else {
                    tracing::warn!(
                        "SVG atlas at max size {}x{} could not fit {}x{} ({} entries, {:.1}% used) — skipping insertion",
                        self.width,
                        self.height,
                        width,
                        height,
                        self.entries.len(),
                        self.utilization() * 100.0,
                    );
                    return None;
                }
            }
        };

        self.used_this_frame.insert(cache_key);
        self.write_pixels(&region, rgba_pixels);
        self.entries.insert(cache_key, region);
        Some(region)
    }

    /// Upload dirty pixels to the GPU texture. No-op when the atlas
    /// hasn't been lazily allocated yet (`upload` runs unconditionally
    /// from the renderer, but the atlas may be untouched).
    pub fn upload(&mut self, queue: &wgpu::Queue) {
        if !self.dirty {
            return;
        }
        let (Some(texture), Some(pixels)) = (self.texture.as_ref(), self.pixels.as_ref()) else {
            return;
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.dirty = false;
    }

    /// Clear all entries and shelves (full eviction). The pixel shadow
    /// buffer is only zeroed if it was ever allocated.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.shelves.clear();
        if let Some(pixels) = self.pixels.as_mut() {
            pixels.fill(0);
        }
        self.used_this_frame.clear();
        self.dirty = true;
    }

    /// Call at the **start** of each SVG render pass, before any
    /// `get()` / `insert()` calls. Evicts entries from the previous
    /// frame that won't be needed, then resets the tracking set for
    /// the new frame.
    ///
    /// Eviction runs here — not reactively inside `insert()` — so
    /// that all UV coordinates computed during the frame stay stable.
    /// A mid-loop repack would move surviving entries to new shelf
    /// positions, invalidating UVs already pushed into the instance
    /// buffer and producing a visible blink.
    pub fn begin_frame(&mut self, device: &wgpu::Device) {
        // Only evict if the atlas is ≥75% full and there are entries
        // from the previous frame that weren't used.
        if self.utilization() >= 0.75 {
            let stale_count = self
                .entries
                .keys()
                .filter(|k| !self.used_this_frame.contains(k))
                .count();
            if stale_count > 0 {
                self.evict_unused(device);
            }
        }
        self.used_this_frame.clear();
    }

    /// Evict entries that weren't accessed this frame, then reset the
    /// allocator so new insertions can reuse the freed space. Returns
    /// `true` if any entries were evicted.
    ///
    /// This is the escape hatch for animated SVGs that produce unique
    /// cache keys every frame (e.g. rotating icons with per-frame
    /// tint changes). Without eviction the atlas fills up permanently
    /// and all subsequent insertions fail.
    fn evict_unused(&mut self, device: &wgpu::Device) -> bool {
        let before = self.entries.len();
        // Keep only entries that were accessed this frame
        self.entries
            .retain(|key, _| self.used_this_frame.contains(key));
        let evicted = before - self.entries.len();
        if evicted == 0 {
            return false;
        }

        tracing::info!(
            "SVG atlas: evicted {} stale entries ({} remain), repacking",
            evicted,
            self.entries.len(),
        );

        // Repack: save the pixel data for surviving entries, clear
        // the atlas, then re-insert them. This reclaims fragmented
        // space from the shelf packer.
        let surviving: Vec<(u64, Vec<u8>, u32, u32)> = {
            let Some(pixels) = self.pixels.as_ref() else {
                return false;
            };
            self.entries
                .iter()
                .map(|(&key, region)| {
                    let row_bytes = region.width as usize * 4;
                    let mut px = vec![0u8; row_bytes * region.height as usize];
                    for y in 0..region.height {
                        let src =
                            ((region.y + y) as usize * self.width as usize + region.x as usize) * 4;
                        let dst = y as usize * row_bytes;
                        if src + row_bytes <= pixels.len() {
                            px[dst..dst + row_bytes].copy_from_slice(&pixels[src..src + row_bytes]);
                        }
                    }
                    (key, px, region.width, region.height)
                })
                .collect()
        };

        // Full reset of the packing state
        self.shelves.clear();
        self.entries.clear();
        if let Some(pixels) = self.pixels.as_mut() {
            pixels.fill(0);
        }

        // Re-insert surviving entries
        for (key, px, w, h) in surviving {
            if let Some(region) = self.allocate(w, h) {
                self.write_pixels(&region, &px);
                self.entries.insert(key, region);
            } else {
                // Atlas shrunk below surviving set — try growing
                if self.grow(device)
                    && let Some(region) = self.allocate(w, h)
                {
                    self.write_pixels(&region, &px);
                    self.entries.insert(key, region);
                }
            }
        }

        self.dirty = true;
        true
    }

    /// Calculate atlas utilization (0.0 to 1.0)
    pub fn utilization(&self) -> f32 {
        let used_height = self.shelves.last().map(|s| s.y + s.height).unwrap_or(0);
        used_height as f32 / self.height as f32
    }

    /// Allocate a region using shelf packing
    fn allocate(&mut self, width: u32, height: u32) -> Option<SvgAtlasRegion> {
        let padded_w = width + PADDING;
        let padded_h = height + PADDING;

        // Find best shelf (smallest height that fits, lowest Y)
        let mut best_shelf: Option<usize> = None;
        let mut best_y = u32::MAX;

        for (i, shelf) in self.shelves.iter().enumerate() {
            if shelf.height >= padded_h && shelf.x + padded_w <= self.width && shelf.y < best_y {
                best_y = shelf.y;
                best_shelf = Some(i);
            }
        }

        if let Some(idx) = best_shelf {
            let shelf = &mut self.shelves[idx];
            let region = SvgAtlasRegion {
                x: shelf.x,
                y: shelf.y,
                width,
                height,
            };
            shelf.x += padded_w;
            return Some(region);
        }

        // Create new shelf
        let new_y = self.shelves.last().map(|s| s.y + s.height).unwrap_or(0);
        if new_y + padded_h > self.height {
            return None;
        }

        let region = SvgAtlasRegion {
            x: 0,
            y: new_y,
            width,
            height,
        };

        self.shelves.push(Shelf {
            y: new_y,
            height: padded_h,
            x: padded_w,
        });

        Some(region)
    }

    /// Blit RGBA pixel data into the atlas at the given region.
    /// Caller must have called `ensure_alloc` first (every public path
    /// to `write_pixels` goes through `insert` which does so).
    fn write_pixels(&mut self, region: &SvgAtlasRegion, rgba: &[u8]) {
        let Some(pixels) = self.pixels.as_mut() else {
            return;
        };
        let row_bytes = region.width as usize * 4;
        for y in 0..region.height {
            let src_offset = y as usize * row_bytes;
            let dst_offset =
                ((region.y + y) as usize * self.width as usize + region.x as usize) * 4;
            if src_offset + row_bytes <= rgba.len() && dst_offset + row_bytes <= pixels.len() {
                pixels[dst_offset..dst_offset + row_bytes]
                    .copy_from_slice(&rgba[src_offset..src_offset + row_bytes]);
            }
        }
        self.dirty = true;
    }

    /// Double atlas dimensions, copy old pixels into top-left quadrant.
    /// Creates a new GPU texture. Returns false if already at max size
    /// or if the atlas hasn't been allocated yet (caller must
    /// `ensure_alloc` before invoking grow).
    fn grow(&mut self, device: &wgpu::Device) -> bool {
        let new_w = (self.width * 2).min(MAX_SIZE);
        let new_h = (self.height * 2).min(MAX_SIZE);

        if new_w == self.width && new_h == self.height {
            return false;
        }

        let Some(old_pixels) = self.pixels.as_ref() else {
            return false;
        };

        let mut new_pixels = vec![0u8; (new_w * new_h * 4) as usize];
        for y in 0..self.height {
            let src_start = (y * self.width * 4) as usize;
            let src_end = src_start + (self.width * 4) as usize;
            let dst_start = (y * new_w * 4) as usize;
            let dst_end = dst_start + (self.width * 4) as usize;
            new_pixels[dst_start..dst_end].copy_from_slice(&old_pixels[src_start..src_end]);
        }

        self.pixels = Some(new_pixels);
        self.width = new_w;
        self.height = new_h;

        let (texture, view) = create_atlas_texture(device, new_w, new_h);
        self.texture = Some(texture);
        self.view = Some(view);
        self.dirty = true;

        tracing::info!(
            "SVG atlas grew to {}x{} ({:.1} MB)",
            new_w,
            new_h,
            (new_w as f64 * new_h as f64 * 4.0) / (1024.0 * 1024.0)
        );
        true
    }
}

fn create_atlas_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SVG Atlas"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
