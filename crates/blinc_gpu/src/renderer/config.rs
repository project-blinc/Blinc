//! Renderer configuration, its environment overrides, the memory budget and the error type.

use super::{Buffers, GpuRenderer};
use crate::primitives::{GpuGlassPrimitive, GpuGlyph, GpuPrimitive};

pub(super) fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
}

pub(super) fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
}

pub(super) fn apply_renderer_config_overrides(
    mut config: RendererConfig,
    required_limits: &wgpu::Limits,
) -> RendererConfig {
    // Allow raising internal buffer capacities at startup.
    // These do NOT change hardware capabilities; they just size our storage buffers.
    //
    // Env:
    // - BLINC_GPU_MAX_PRIMITIVES=20000
    // - BLINC_GPU_MAX_GLYPHS=50000
    // - BLINC_GPU_MAX_GLASS_PRIMITIVES=1000
    if let Some(v) = env_usize("BLINC_GPU_MAX_PRIMITIVES") {
        config.max_primitives = v;
    }
    if let Some(v) = env_usize("BLINC_GPU_MAX_GLYPHS") {
        config.max_glyphs = v;
    }
    if let Some(v) = env_usize("BLINC_GPU_MAX_GLASS_PRIMITIVES") {
        config.max_glass_primitives = v;
    }

    // Clamp to required limits so device creation + bind sizes stay valid.
    let prim_cap = (required_limits.max_storage_buffer_binding_size as u64
        / std::mem::size_of::<GpuPrimitive>() as u64)
        .max(1) as usize;
    let glyph_cap = (required_limits.max_storage_buffer_binding_size as u64
        / std::mem::size_of::<GpuGlyph>() as u64)
        .max(1) as usize;
    let glass_cap = (required_limits.max_storage_buffer_binding_size as u64
        / std::mem::size_of::<GpuGlassPrimitive>() as u64)
        .max(1) as usize;

    config.max_primitives = config.max_primitives.clamp(1, prim_cap);
    config.max_glyphs = config.max_glyphs.clamp(1, glyph_cap);
    config.max_glass_primitives = config.max_glass_primitives.clamp(1, glass_cap);

    config
}

pub(super) fn log_renderer_config(config: &RendererConfig) {
    tracing::info!(
        "gpu config: max_primitives={}, max_glyphs={}, max_glass_primitives={}, sample_count={}",
        config.max_primitives,
        config.max_glyphs,
        config.max_glass_primitives,
        config.sample_count
    );
}

/// Error type for renderer operations
#[derive(Debug)]
pub enum RendererError {
    /// Failed to request GPU adapter
    AdapterNotFound,
    /// Failed to request GPU device
    DeviceError(wgpu::RequestDeviceError),
    /// Failed to create surface
    SurfaceError(wgpu::CreateSurfaceError),
    /// Shader compilation error
    ShaderError(String),
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RendererError::AdapterNotFound => write!(f, "No suitable GPU adapter found"),
            RendererError::DeviceError(e) => write!(f, "Failed to request GPU device: {}", e),
            RendererError::SurfaceError(e) => write!(f, "Failed to create surface: {}", e),
            RendererError::ShaderError(e) => write!(f, "Shader compilation error: {}", e),
        }
    }
}

impl std::error::Error for RendererError {}

/// Configuration for creating a renderer
#[derive(Clone, Debug)]
pub struct RendererConfig {
    /// Maximum number of primitives per batch
    pub max_primitives: usize,
    /// Maximum number of glass primitives per batch
    pub max_glass_primitives: usize,
    /// Maximum number of glyphs per batch
    pub max_glyphs: usize,
    /// Enable MSAA (sample count)
    pub sample_count: u32,
    /// Preferred texture format (None = use surface preferred)
    pub texture_format: Option<wgpu::TextureFormat>,
    /// Enable unified text/SDF rendering (renders text as SDF primitives in same pass)
    ///
    /// When enabled, text glyphs are converted to SDF primitives and rendered
    /// in the same GPU pass as other shapes. This ensures consistent transform
    /// timing during animations, preventing visual lag when parent containers
    /// have motion transforms applied.
    ///
    /// Default: true (unified rendering for consistent animations)
    pub unified_text_rendering: bool,
    /// GPU texture memory budget in bytes.
    ///
    /// When total tracked texture memory exceeds this budget, the renderer
    /// evicts least-recently-used textures from caches. Set to 0 to disable.
    ///
    /// Default: 128 MB. Override with `BLINC_GPU_MEMORY_BUDGET_MB` env var.
    pub gpu_memory_budget: u64,
    /// Optional path to a persisted `PipelineCache` file. When set and the
    /// adapter supports `Features::PIPELINE_CACHE`, the renderer seeds the
    /// pipeline cache from this file (validated against the current adapter)
    /// and writes it back after building the core pipelines, so every launch
    /// after the first skips the bulk of pipeline (re)creation. Platform
    /// runners point this at a per-app writable dir (e.g. the Android
    /// internal data path). `None` disables the persisted cache.
    ///
    /// Default: `None`.
    pub pipeline_cache_path: Option<std::path::PathBuf>,
}

impl Default for RendererConfig {
    fn default() -> Self {
        let budget_mb: u64 = std::env::var("BLINC_GPU_MEMORY_BUDGET_MB")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(128);

        Self {
            // Conservative defaults for low memory footprint
            // Buffers are re-created if scenes exceed these limits, so no hard cap
            max_primitives: 20_000,   // ~400 KB — handles complex UI screens
            max_glass_primitives: 32, // ~8 KB
            max_glyphs: 4_000,        // ~256 KB — handles full-screen text content
            sample_count: 1,
            texture_format: None,
            unified_text_rendering: true, // Enabled for consistent transforms during animations
            gpu_memory_budget: budget_mb * 1024 * 1024,
            pipeline_cache_path: None,
        }
    }
}

/// Tracks GPU texture memory usage across all caches and enforces a budget.
pub struct GpuMemoryBudget {
    /// Maximum allowed texture memory in bytes (0 = unlimited)
    pub(super) budget: u64,
    /// Memory used by mask image cache
    pub(super) mask_image_bytes: u64,
    /// Memory used by mesh textures (transient, per-frame)
    pub(super) mesh_texture_bytes: u64,
    /// Number of eviction passes performed
    pub(super) eviction_count: u64,
}

impl GpuMemoryBudget {
    pub fn new(budget: u64) -> Self {
        Self {
            budget,
            mask_image_bytes: 0,
            mesh_texture_bytes: 0,
            eviction_count: 0,
        }
    }

    /// Report current total tracked memory across all sources.
    pub fn total_tracked_bytes(&self, layer_cache_bytes: u64) -> u64 {
        layer_cache_bytes + self.mask_image_bytes + self.mesh_texture_bytes
    }

    /// Check if we're over budget.
    pub fn is_over_budget(&self, layer_cache_bytes: u64) -> bool {
        self.budget > 0 && self.total_tracked_bytes(layer_cache_bytes) > self.budget
    }

    /// Track a mask image being added to the cache.
    pub fn track_mask_image(&mut self, width: u32, height: u32) {
        self.mask_image_bytes += (width as u64) * (height as u64) * 4;
    }

    /// Track a mask image being removed from the cache.
    pub fn untrack_mask_image(&mut self, width: u32, height: u32) {
        let bytes = (width as u64) * (height as u64) * 4;
        self.mask_image_bytes = self.mask_image_bytes.saturating_sub(bytes);
    }

    /// Reset per-frame transient tracking (mesh textures, etc.)
    pub fn reset_transient(&mut self) {
        self.mesh_texture_bytes = 0;
    }

    /// Get the memory budget in bytes.
    pub fn budget(&self) -> u64 {
        self.budget
    }

    /// Get number of eviction passes performed.
    pub fn eviction_count(&self) -> u64 {
        self.eviction_count
    }

    /// Increment eviction counter.
    pub fn record_eviction(&mut self) {
        self.eviction_count += 1;
    }
}
