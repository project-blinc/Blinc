//! Offscreen layer textures and the cache that pools them by size.

use super::GpuRenderer;
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// Layer Texture Management
// ─────────────────────────────────────────────────────────────────────────────

/// A texture used for offscreen layer rendering
///
/// Layer textures are used for rendering layers to offscreen targets,
/// enabling layer composition with blend modes and effects.
pub struct LayerTexture {
    /// The GPU texture for color data
    pub texture: wgpu::Texture,
    /// View into the texture for rendering
    pub view: wgpu::TextureView,
    /// Size of the texture in pixels (width, height)
    pub size: (u32, u32),
    /// Whether this texture has an associated depth buffer
    pub has_depth: bool,
    /// Optional depth texture view (for 3D content)
    pub depth_view: Option<wgpu::TextureView>,
    /// Optional depth texture (kept alive for the view)
    pub(super) depth_texture: Option<wgpu::Texture>,
}

impl LayerTexture {
    /// Create a new layer texture with the given size
    pub fn new(
        device: &wgpu::Device,
        size: (u32, u32),
        format: wgpu::TextureFormat,
        with_depth: bool,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("layer_texture"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let (depth_texture, depth_view) = if with_depth {
            let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("layer_depth_texture"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());
            (Some(depth_tex), Some(depth_view))
        } else {
            (None, None)
        };

        Self {
            texture,
            view,
            size,
            has_depth: with_depth,
            depth_view,
            depth_texture,
        }
    }

    /// Check if this texture matches the requested size
    pub fn matches_size(&self, size: (u32, u32)) -> bool {
        self.size == size
    }
}

/// Size bucket for texture pooling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureSizeBucket {
    Small,  // <= 128
    Medium, // <= 256
    Large,  // <= 512
    XLarge, // > 512 (not pooled by default)
}

impl TextureSizeBucket {
    /// Get the bucket for a given size
    pub(super) fn from_size(size: (u32, u32)) -> Self {
        let max_dim = size.0.max(size.1);
        if max_dim <= 128 {
            Self::Small
        } else if max_dim <= 256 {
            Self::Medium
        } else if max_dim <= 512 {
            Self::Large
        } else {
            Self::XLarge
        }
    }

    /// Get the maximum size for this bucket (for rounding up)
    pub(super) fn max_size(&self) -> u32 {
        match self {
            Self::Small => 128,
            Self::Medium => 256,
            Self::Large => 512,
            Self::XLarge => u32::MAX,
        }
    }
}

/// Statistics for texture cache performance monitoring
#[derive(Debug, Default, Clone)]
pub struct TextureCacheStats {
    /// Number of cache hits (texture reused from pool)
    pub hits: u64,
    /// Number of cache misses (new texture allocated)
    pub misses: u64,
    /// Number of textures currently in pool
    pub pool_count: usize,
    /// Estimated memory in pool (bytes)
    pub pool_memory_bytes: u64,
    /// Number of named textures
    pub named_count: usize,
    /// Estimated memory in named textures (bytes)
    pub named_memory_bytes: u64,
}

impl TextureCacheStats {
    /// Total estimated memory usage
    pub fn total_memory_bytes(&self) -> u64 {
        self.pool_memory_bytes + self.named_memory_bytes
    }

    /// Cache hit rate (0.0 - 1.0)
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Cache for managing layer textures with size-bucketed pooling
///
/// Implements texture pooling to avoid frequent allocations during rendering.
/// Textures are acquired for layer rendering and released back to the pool
/// when no longer needed. Uses size buckets for more efficient reuse.
pub struct LayerTextureCache {
    /// Map of layer IDs to their dedicated textures
    pub(super) named_textures: std::collections::HashMap<blinc_core::LayerId, LayerTexture>,
    /// Size-bucketed pools for efficient texture reuse
    pub(super) pool_small: Vec<LayerTexture>, // <= 128
    pub(super) pool_medium: Vec<LayerTexture>, // <= 256
    pub(super) pool_large: Vec<LayerTexture>,  // <= 512
    pub(super) pool_xlarge: Vec<LayerTexture>, // > 512
    /// Texture format used for all layer textures
    pub(super) format: wgpu::TextureFormat,
    /// Maximum textures per bucket
    pub(super) max_per_bucket: usize,
    /// Cache statistics
    pub(super) stats: TextureCacheStats,
    /// Number of consecutive frames without a successful `acquire()`.
    /// `evict_oversized()` (called once per frame from the renderer)
    /// increments this; `acquire()` resets it. After
    /// `IDLE_DROP_THRESHOLD` frames, drop one texture from each pool.
    /// After `IDLE_FULL_FLUSH` frames, drop everything. This reclaims
    /// the GPU memory held by layer/glass/blur intermediates when the
    /// UI hasn't needed them for a while.
    pub(super) idle_frames: u32,
}

/// Drop one texture per pool after this many idle frames (~1 s at 60fps).
const IDLE_DROP_THRESHOLD: u32 = 60;
/// Drop the entire pool after this many idle frames (~2 s at 60fps).
pub(super) const IDLE_FULL_FLUSH: u32 = 120;

impl LayerTextureCache {
    /// Create a new layer texture cache
    pub fn new(format: wgpu::TextureFormat) -> Self {
        Self {
            named_textures: std::collections::HashMap::new(),
            pool_small: Vec::with_capacity(2),
            pool_medium: Vec::with_capacity(2),
            pool_large: Vec::with_capacity(2),
            pool_xlarge: Vec::with_capacity(2),
            format,
            max_per_bucket: 2,
            stats: TextureCacheStats::default(),
            idle_frames: 0,
        }
    }

    /// Estimate memory usage of a texture in bytes (RGBA8 = 4 bytes per pixel)
    pub(super) fn estimate_texture_bytes(size: (u32, u32), has_depth: bool) -> u64 {
        let color_bytes = (size.0 as u64) * (size.1 as u64) * 4;
        let depth_bytes = if has_depth {
            (size.0 as u64) * (size.1 as u64) * 4 // Depth32Float = 4 bytes
        } else {
            0
        };
        color_bytes + depth_bytes
    }

    /// Get the appropriate pool for a bucket
    pub(super) fn get_pool(&self, bucket: TextureSizeBucket) -> &Vec<LayerTexture> {
        match bucket {
            TextureSizeBucket::Small => &self.pool_small,
            TextureSizeBucket::Medium => &self.pool_medium,
            TextureSizeBucket::Large => &self.pool_large,
            TextureSizeBucket::XLarge => &self.pool_xlarge,
        }
    }

    /// Get mutable pool for a bucket
    pub(super) fn get_pool_mut(&mut self, bucket: TextureSizeBucket) -> &mut Vec<LayerTexture> {
        match bucket {
            TextureSizeBucket::Small => &mut self.pool_small,
            TextureSizeBucket::Medium => &mut self.pool_medium,
            TextureSizeBucket::Large => &mut self.pool_large,
            TextureSizeBucket::XLarge => &mut self.pool_xlarge,
        }
    }

    /// Acquire a texture of at least the given size
    ///
    /// First checks the pool for a matching texture, otherwise creates a new one.
    /// Textures may be larger than requested (rounded up to bucket size).
    pub fn acquire(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
        with_depth: bool,
    ) -> LayerTexture {
        let bucket = TextureSizeBucket::from_size(size);

        // Helper to find a matching texture in a pool
        fn find_matching(
            pool: &[LayerTexture],
            size: (u32, u32),
            with_depth: bool,
        ) -> Option<usize> {
            pool.iter()
                .position(|t| t.size.0 >= size.0 && t.size.1 >= size.1 && t.has_depth == with_depth)
        }

        // Try to find in primary bucket
        let primary_pool = match bucket {
            TextureSizeBucket::Small => &self.pool_small,
            TextureSizeBucket::Medium => &self.pool_medium,
            TextureSizeBucket::Large => &self.pool_large,
            TextureSizeBucket::XLarge => &self.pool_xlarge,
        };
        let found_in_primary = find_matching(primary_pool, size, with_depth);

        if let Some(index) = found_in_primary {
            self.stats.hits += 1;
            self.idle_frames = 0;
            let texture = match bucket {
                TextureSizeBucket::Small => self.pool_small.swap_remove(index),
                TextureSizeBucket::Medium => self.pool_medium.swap_remove(index),
                TextureSizeBucket::Large => self.pool_large.swap_remove(index),
                TextureSizeBucket::XLarge => self.pool_xlarge.swap_remove(index),
            };
            self.update_pool_stats();
            return texture;
        }

        // Try larger buckets as fallback
        let found_in_larger = match bucket {
            TextureSizeBucket::Small => find_matching(&self.pool_medium, size, with_depth)
                .map(|i| (TextureSizeBucket::Medium, i))
                .or_else(|| {
                    find_matching(&self.pool_large, size, with_depth)
                        .map(|i| (TextureSizeBucket::Large, i))
                }),
            TextureSizeBucket::Medium => find_matching(&self.pool_large, size, with_depth)
                .map(|i| (TextureSizeBucket::Large, i)),
            _ => None,
        };

        if let Some((larger_bucket, index)) = found_in_larger {
            self.stats.hits += 1;
            self.idle_frames = 0;
            let texture = match larger_bucket {
                TextureSizeBucket::Medium => self.pool_medium.swap_remove(index),
                TextureSizeBucket::Large => self.pool_large.swap_remove(index),
                _ => unreachable!(),
            };
            self.update_pool_stats();
            return texture;
        }

        // No suitable texture in pool, create a new one
        self.stats.misses += 1;
        self.idle_frames = 0;

        // Round up for better future reuse
        let rounded_size = if bucket == TextureSizeBucket::XLarge {
            // Round XLarge to 64px increments for better cache reuse
            let w = size.0.div_ceil(64) * 64;
            let h = size.1.div_ceil(64) * 64;
            (w, h)
        } else {
            let bucket_max = bucket.max_size();
            (size.0.max(bucket_max), size.1.max(bucket_max))
        };

        LayerTexture::new(device, rounded_size, self.format, with_depth)
    }

    /// Acquire an exact extent for passes that sample the whole input.
    pub(super) fn acquire_exact(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
        with_depth: bool,
    ) -> LayerTexture {
        let bucket = TextureSizeBucket::from_size(size);
        let pool = self.get_pool_mut(bucket);
        if let Some(index) = pool
            .iter()
            .position(|t| t.size == size && t.has_depth == with_depth)
        {
            let texture = pool.swap_remove(index);
            self.stats.hits += 1;
            self.idle_frames = 0;
            self.update_pool_stats();
            return texture;
        }
        self.stats.misses += 1;
        self.idle_frames = 0;
        LayerTexture::new(device, size, self.format, with_depth)
    }

    /// Release a texture back to the pool
    ///
    /// If the pool bucket is full or the texture is too large, it's dropped.
    pub fn release(&mut self, texture: LayerTexture) {
        let bucket = TextureSizeBucket::from_size(texture.size);
        let max = self.max_per_bucket;

        let pool = match bucket {
            TextureSizeBucket::Small => &mut self.pool_small,
            TextureSizeBucket::Medium => &mut self.pool_medium,
            TextureSizeBucket::Large => &mut self.pool_large,
            TextureSizeBucket::XLarge => &mut self.pool_xlarge,
        };

        if pool.len() < max {
            pool.push(texture);
            self.update_pool_stats();
        }
        // Otherwise let the texture be dropped
    }

    /// Update pool statistics
    pub(super) fn update_pool_stats(&mut self) {
        let mut count = 0;
        let mut bytes = 0u64;

        for pool in [
            &self.pool_small,
            &self.pool_medium,
            &self.pool_large,
            &self.pool_xlarge,
        ] {
            for t in pool {
                count += 1;
                bytes += Self::estimate_texture_bytes(t.size, t.has_depth);
            }
        }

        self.stats.pool_count = count;
        self.stats.pool_memory_bytes = bytes;
    }

    /// Clear oversized textures from the pool
    ///
    /// Call this at frame start to evict any large textures that accumulated.
    /// Also drives idle-frame eviction: pools that haven't been used in
    /// `IDLE_DROP_THRESHOLD`+ frames shrink by one per frame; after
    /// `IDLE_FULL_FLUSH` they're emptied entirely. This reclaims GPU
    /// memory held by glass/blur intermediates when the UI is sitting
    /// still.
    pub fn evict_oversized(&mut self) {
        // Trim pools that are over capacity
        while self.pool_small.len() > self.max_per_bucket {
            self.pool_small.pop();
        }
        while self.pool_medium.len() > self.max_per_bucket {
            self.pool_medium.pop();
        }
        while self.pool_large.len() > self.max_per_bucket {
            self.pool_large.pop();
        }
        while self.pool_xlarge.len() > self.max_per_bucket {
            self.pool_xlarge.pop();
        }

        // Idle-frame eviction. Saturating add keeps us at u32::MAX after
        // long idle without overflow, which is fine — the comparison
        // against thresholds still holds.
        self.idle_frames = self.idle_frames.saturating_add(1);
        if self.idle_frames >= IDLE_FULL_FLUSH {
            // Drop everything pooled. Next frame's `acquire` will pay
            // a single allocation. XLarge is the priority — biggest
            // memory win.
            self.pool_xlarge.clear();
            self.pool_large.clear();
            self.pool_medium.clear();
            self.pool_small.clear();
        } else if self.idle_frames >= IDLE_DROP_THRESHOLD {
            // Drop one entry per pool per frame. Largest first.
            self.pool_xlarge.pop();
            self.pool_large.pop();
            self.pool_medium.pop();
            self.pool_small.pop();
        }

        self.update_pool_stats();
    }

    /// Evict pooled textures until memory usage drops below `target_bytes`.
    ///
    /// Evicts largest textures first (XLarge → Large → Medium → Small).
    /// Returns the number of bytes freed.
    pub fn evict_to_budget(&mut self, target_bytes: u64) -> u64 {
        let mut freed = 0u64;
        let pools = [
            TextureSizeBucket::XLarge,
            TextureSizeBucket::Large,
            TextureSizeBucket::Medium,
            TextureSizeBucket::Small,
        ];

        for bucket in pools {
            while self.stats.pool_memory_bytes > target_bytes {
                let pool = self.get_pool_mut(bucket);
                if let Some(tex) = pool.pop() {
                    let bytes = Self::estimate_texture_bytes(tex.size, tex.has_depth);
                    freed += bytes;
                    self.update_pool_stats();
                } else {
                    break;
                }
            }
        }
        freed
    }

    /// Store a texture with a layer ID for later retrieval
    pub fn store(&mut self, id: blinc_core::LayerId, texture: LayerTexture) {
        self.named_textures.insert(id, texture);
        self.update_named_stats();
    }

    /// Get a reference to a named layer's texture
    pub fn get(&self, id: &blinc_core::LayerId) -> Option<&LayerTexture> {
        self.named_textures.get(id)
    }

    /// Remove and return a named layer's texture
    pub fn remove(&mut self, id: &blinc_core::LayerId) -> Option<LayerTexture> {
        let result = self.named_textures.remove(id);
        self.update_named_stats();
        result
    }

    /// Update named texture statistics
    pub(super) fn update_named_stats(&mut self) {
        let mut bytes = 0u64;
        for t in self.named_textures.values() {
            bytes += Self::estimate_texture_bytes(t.size, t.has_depth);
        }
        self.stats.named_count = self.named_textures.len();
        self.stats.named_memory_bytes = bytes;
    }

    /// Clear all named textures (releases them to pool or drops them)
    pub fn clear_named(&mut self) {
        let textures: Vec<_> = self.named_textures.drain().map(|(_, t)| t).collect();
        for texture in textures {
            self.release(texture);
        }
        self.update_named_stats();
    }

    /// Drop the pooled textures, keeping the named ones.
    pub fn clear_pools(&mut self) {
        self.pool_small.clear();
        self.pool_medium.clear();
        self.pool_large.clear();
        self.pool_xlarge.clear();
        self.update_pool_stats();
    }

    /// Clear the entire cache including pool
    pub fn clear_all(&mut self) {
        self.named_textures.clear();
        self.pool_small.clear();
        self.pool_medium.clear();
        self.pool_large.clear();
        self.pool_xlarge.clear();
        self.stats = TextureCacheStats::default();
    }

    /// Get the total number of textures in all pools
    pub fn pool_size(&self) -> usize {
        self.pool_small.len()
            + self.pool_medium.len()
            + self.pool_large.len()
            + self.pool_xlarge.len()
    }

    /// Get the number of named textures
    pub fn named_count(&self) -> usize {
        self.named_textures.len()
    }

    /// Get current cache statistics
    pub fn stats(&self) -> &TextureCacheStats {
        &self.stats
    }

    /// Reset cache statistics (call at start of profiling)
    pub fn reset_stats(&mut self) {
        self.stats.hits = 0;
        self.stats.misses = 0;
        self.update_pool_stats();
        self.update_named_stats();
    }
}
