//! GPU renderer implementation
//!
//! The main renderer that manages wgpu resources and executes render passes
//! for SDF primitives, glass effects, and text.
//!
//! ## A note on wasm32 + `Arc`
//!
//! On `wasm32-unknown-unknown` the wgpu API is single-threaded by design
//! (the WebGPU JavaScript interface lives on the main browser thread),
//! so `wgpu::Device` and `wgpu::Queue` are `!Send + !Sync`. Wrapping them
//! in `Arc` is still the right call — every other Blinc subsystem uses
//! `Arc<Device>` / `Arc<Queue>` to share GPU handles, and the
//! alternative (per-target `Rc` vs `Arc` aliases) would leak through
//! every storage site in `blinc_gpu`, `blinc_app::context`, the text
//! renderer, etc. Clippy's `arc_with_non_send_sync` lint catches the
//! theoretical footgun that an `Arc` of a `!Send` type can never
//! actually be sent across threads, but on wasm32 there are no other
//! threads to send to. The lint is `allow`ed at the module level for
//! that target only.

#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;

mod dynamic_image;
mod foreground;
mod glass;
mod idle;
mod image_blend;
mod mesh_dispatch;
mod overlay;
mod overlay_targets;
use image_blend::ImageBlend;
mod bind_group_layouts;
mod buffers;
mod construction;
mod effect_pipelines;
mod effects;
mod flow;
mod frame;
mod glass_frame;
mod images;
mod layer_cache;
mod layers;
mod overlay_msaa;
mod overlay_render;
mod pipelines;
mod sdf_dispatch;
mod static_layer;
mod text;
pub use layer_cache::{LayerTexture, LayerTextureCache, TextureCacheStats, TextureSizeBucket};

use crate::gradient_texture::GradientTextureCache;
use crate::image::GpuImageInstance;
use crate::path::PathVertex;
use crate::primitives::{
    BlurUniforms, ColorMatrixUniforms, DropShadowUniforms, GlassType, GlassUniforms, GlowUniforms,
    GpuGlassPrimitive, GpuGlyph, GpuPrimitive, MaskImageUniforms, PathUniforms, PrimitiveBatch,
    Sdf3DUniform, SdfPipelineCategory, SdfVertexInstance, Uniforms, Viewport3D,
};
use crate::shaders::{
    BLUR_SHADER, CLEAR_QUAD_SHADER, COLOR_MATRIX_SHADER, COMPOSITE_SHADER, DROP_SHADOW_SHADER,
    GLASS_DT_SHADER, GLASS_SHADER, GLOW_SHADER, IMAGE_DT_SHADER, IMAGE_SHADER,
    LAYER_COMPOSITE_SHADER, MASK_IMAGE_SHADER, MESH_DT_SHADER, PATH_DT_SHADER, PATH_SHADER,
    SDF_3D_DT_SHADER, SDF_3D_SHADER, SDF_3D_VB_SHADER, SDF_CORE_DT_SHADER, SDF_CORE_SHADER,
    SDF_CORE_VB_SHADER, SDF_NOTCH_DT_SHADER, SDF_NOTCH_SHADER, SDF_NOTCH_VB_SHADER,
    SDF_SHADOW_DT_SHADER, SDF_SHADOW_SHADER, SDF_SHADOW_VB_SHADER, SIMPLE_GLASS_DT_SHADER,
    SIMPLE_GLASS_SHADER, TEXT_DT_SHADER, TEXT_SHADER,
};

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
}

/// Feature set to request from `adapter.request_device`. Picks
/// `TEXTURE_COMPRESSION_BC` when the adapter advertises it; the
/// renderer probes the active feature set afterward via
/// `device.features()` to decide whether to upload BC-encoded
/// material textures or fall back to Rgba8.
///
/// Safe to call on wgpu backends that don't support BC (WebGL2,
/// iOS/Metal-without-BC): the adapter's features query simply
/// returns `false` and no feature is requested. Callers that
/// need to detect this downstream should query
/// `GpuRenderer::has_texture_compression_bc()`.
fn requested_device_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    let mut features = wgpu::Features::empty();
    let available = adapter.features();
    if available.contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
        features |= wgpu::Features::TEXTURE_COMPRESSION_BC;
    }
    // Persisted pipeline cache: on mobile, rebuilding all render pipelines
    // from scratch every launch is the dominant cold-start cost (measured
    // ~925ms for the core pipelines on a Pixel 10 Pro). A serialized
    // `PipelineCache` collapses that on every launch after the first.
    if available.contains(wgpu::Features::PIPELINE_CACHE) {
        features |= wgpu::Features::PIPELINE_CACHE;
    }
    features
}

// On-disk pipeline-cache header. Bump the version when the layout changes.
const PIPELINE_CACHE_MAGIC: &[u8; 4] = b"BLPC";
const PIPELINE_CACHE_VERSION: u32 = 1;
// magic(4) + version(4) + vendor(4) + device(4) + driver_hash(8)
// + data_len(8) + data_hash(8)
const PIPELINE_CACHE_HEADER_LEN: usize = 40;

fn stable_hash_str(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn stable_hash_bytes(b: &[u8]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    h.write(b);
    h.finish()
}

fn pipeline_cache_driver_key(info: &wgpu::AdapterInfo) -> u64 {
    stable_hash_str(&format!("{}|{}", info.driver, info.driver_info))
}

/// Load previously-serialized `PipelineCache` data, validated against the
/// current adapter. Android drivers do minimal integrity checking on cache
/// blobs, so we gate the data behind a header (magic, layout version,
/// vendor/device IDs, driver string hash, and a hash of the payload) and
/// hand nothing to the driver unless every field matches. `fallback: true`
/// on the descriptor is the second safety net.
fn load_pipeline_cache_data(path: &std::path::Path, info: &wgpu::AdapterInfo) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < PIPELINE_CACHE_HEADER_LEN || &bytes[0..4] != PIPELINE_CACHE_MAGIC {
        return None;
    }
    let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let u64_at = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
    if u32_at(4) != PIPELINE_CACHE_VERSION
        || u32_at(8) != info.vendor
        || u32_at(12) != info.device
        || u64_at(16) != pipeline_cache_driver_key(info)
    {
        return None;
    }
    let data_len = u64_at(24) as usize;
    let data_hash = u64_at(32);
    let data = bytes.get(PIPELINE_CACHE_HEADER_LEN..PIPELINE_CACHE_HEADER_LEN + data_len)?;
    if stable_hash_bytes(data) != data_hash {
        return None;
    }
    Some(data.to_vec())
}

/// Serialize `PipelineCache` data with the validation header, keyed to the
/// current adapter. Best-effort: write to a temp file and rename so a
/// half-written cache is never read on the next launch.
fn save_pipeline_cache_data(path: &std::path::Path, info: &wgpu::AdapterInfo, data: &[u8]) {
    let mut out = Vec::with_capacity(PIPELINE_CACHE_HEADER_LEN + data.len());
    out.extend_from_slice(PIPELINE_CACHE_MAGIC);
    out.extend_from_slice(&PIPELINE_CACHE_VERSION.to_le_bytes());
    out.extend_from_slice(&info.vendor.to_le_bytes());
    out.extend_from_slice(&info.device.to_le_bytes());
    out.extend_from_slice(&pipeline_cache_driver_key(info).to_le_bytes());
    out.extend_from_slice(&(data.len() as u64).to_le_bytes());
    out.extend_from_slice(&stable_hash_bytes(data).to_le_bytes());
    out.extend_from_slice(data);
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, &out).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

fn device_required_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
    // Default wgpu limits include `max_buffer_size = 256 MiB`.
    // This is conservative and may be smaller than what the hardware supports.
    //
    // If you want to raise this limit (e.g. for large path buffers), set:
    //   BLINC_WGPU_MAX_BUFFER_MB=512
    // The value is clamped to the adapter-supported maximum.
    let supported = adapter.limits();

    // On wasm32, use the adapter's own supported limits directly
    // instead of requesting wgpu defaults. Different browsers support
    // different subsets of WebGPU — Safari/Firefox may report 0 for
    // compute workgroups or storage buffer binding size. Requesting
    // any limit above what the adapter supports causes device creation
    // to fail. Using the adapter's limits verbatim is always safe.
    #[cfg(target_arch = "wasm32")]
    let mut limits = supported.clone();
    #[cfg(not(target_arch = "wasm32"))]
    let mut limits = wgpu::Limits::default();

    if let Some(mib) = env_u64("BLINC_WGPU_MAX_BUFFER_MB") {
        let requested = mib.saturating_mul(1024 * 1024);
        let clamped = requested.min(supported.max_buffer_size);
        limits.max_buffer_size = clamped;

        tracing::info!(
            "wgpu limits override: max_buffer_size={} MiB (requested {} MiB, supported {} MiB)",
            limits.max_buffer_size / (1024 * 1024),
            mib,
            supported.max_buffer_size / (1024 * 1024)
        );
    } else {
        tracing::debug!(
            "wgpu limits: max_buffer_size={} MiB (supported {} MiB)",
            limits.max_buffer_size / (1024 * 1024),
            supported.max_buffer_size / (1024 * 1024)
        );
    }

    limits
}

fn apply_renderer_config_overrides(
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

fn log_renderer_config(config: &RendererConfig) {
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

/// Axis-aligned bounding box intersection test. Each rect is
/// `[x, y, w, h]` (top-left origin, screen pixels). Returns true when
/// the rects overlap (zero-area overlap counts as no intersection).
fn aabb_intersects(a: [f32; 4], b: [f32; 4]) -> bool {
    let [ax, ay, aw, ah] = a;
    let [bx, by, bw, bh] = b;
    ax + aw > bx && bx + bw > ax && ay + ah > by && by + bh > ay
}

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

// ─────────────────────────────────────────────────────────────────────────────
// GPU Memory Budget & Eviction
// ─────────────────────────────────────────────────────────────────────────────

/// Tracks GPU texture memory usage across all caches and enforces a budget.
pub struct GpuMemoryBudget {
    /// Maximum allowed texture memory in bytes (0 = unlimited)
    budget: u64,
    /// Memory used by mask image cache
    mask_image_bytes: u64,
    /// Memory used by mesh textures (transient, per-frame)
    mesh_texture_bytes: u64,
    /// Number of eviction passes performed
    eviction_count: u64,
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

/// Render pipelines for different primitive types
/// One stack frame for a pending `LayerCommand::Push`, holding the
/// primitive / path indices at push time so the matching Pop can
/// compute full ranges.
#[derive(Copy, Clone)]
struct LayerStackFrame {
    primitive_start: usize,
    path_index_start: usize,
    path_vertex_start: usize,
}

/// A resolved effect layer (between Push and Pop) with its primitive
/// range and path range. Processed by `render_with_layer_effects` to
/// either skip its content in the first pass or composite it
/// offscreen in the second pass.
#[derive(Clone)]
struct EffectLayerRange {
    primitive_start: usize,
    primitive_end: usize,
    path_index_start: usize,
    path_index_end: usize,
    path_vertex_start: usize,
    path_vertex_end: usize,
    config: blinc_core::LayerConfig,
}

struct Pipelines {
    /// Split SDF pipeline: core shapes (Rect, Circle, Ellipse)
    sdf_core: wgpu::RenderPipeline,
    /// Split SDF pipeline: shadow shapes (Shadow, InnerShadow, CircleShadow, CircleInnerShadow)
    sdf_shadow: wgpu::RenderPipeline,
    /// Split SDF pipeline: 3D raymarched shapes
    sdf_3d: wgpu::RenderPipeline,
    /// Split SDF pipeline: notch shapes
    sdf_notch: wgpu::RenderPipeline,
    /// Split SDF overlay pipeline: core shapes (1x sampled)
    sdf_core_overlay: wgpu::RenderPipeline,
    /// Core shapes that use none of the optional features (the core shader compiled without
    /// them). Shares the full pipeline on tiers whose shader has no feature switches.
    sdf_core_simple: wgpu::RenderPipeline,
    /// The 1x sampled overlay of `sdf_core_simple`.
    sdf_core_simple_overlay: wgpu::RenderPipeline,
    /// Plain rects and circles through the compact shader. Shares `sdf_core_simple` on tiers
    /// without storage buffers in the vertex stage.
    sdf_plain: wgpu::RenderPipeline,
    /// The 1x sampled overlay of `sdf_plain`.
    sdf_plain_overlay: wgpu::RenderPipeline,
    /// Split SDF overlay pipeline: shadow shapes (1x sampled)
    sdf_shadow_overlay: wgpu::RenderPipeline,
    /// Split SDF overlay pipeline: 3D raymarched shapes (1x sampled)
    sdf_3d_overlay: wgpu::RenderPipeline,
    /// Split SDF overlay pipeline: notch shapes (1x sampled)
    sdf_notch_overlay: wgpu::RenderPipeline,
    /// Pipeline for text rendering on top of existing content (1x sampled)
    text_overlay: wgpu::RenderPipeline,
    /// Pipeline for final compositing (MSAA)
    composite: wgpu::RenderPipeline,
    /// Pipeline for final compositing (1x sampled, for overlay blending)
    composite_overlay: wgpu::RenderPipeline,
    /// Pipeline for tessellated path rendering
    path: wgpu::RenderPipeline,
    /// Pipeline for tessellated path overlay (1x sampled)
    path_overlay: wgpu::RenderPipeline,
    /// Pipeline for layer composition (blend modes)
    layer_composite: wgpu::RenderPipeline,
    /// Compositor v2 damage-rect scissored clear. Draws a fullscreen
    /// triangle with REPLACE blend writing `(0,0,0,0)`; combined with
    /// `set_scissor_rect`, only the damaged region is zeroed. Used by
    /// `render_static_layer_damaged` to clear ghost pixels from the
    /// motion-bound element's previous position before the SDF
    /// dispatch re-paints over it.
    clear_quad: wgpu::RenderPipeline,
}

/// Effect pipelines lazily created on first use to reduce GPU memory for simple apps
struct EffectPipelines {
    /// Pipeline for Kawase blur effect
    blur: Option<wgpu::RenderPipeline>,
    /// Pipeline for color matrix transformation
    color_matrix: Option<wgpu::RenderPipeline>,
    /// Pipeline for drop shadow effect
    drop_shadow: Option<wgpu::RenderPipeline>,
    /// Pipeline for glow effect
    glow: Option<wgpu::RenderPipeline>,
    /// Pipeline for mask image effect
    mask_image: Option<wgpu::RenderPipeline>,
    /// Pipeline for glass/vibrancy effects (liquid glass with refraction)
    glass: Option<wgpu::RenderPipeline>,
    /// Pipeline for simple frosted glass (pure blur, no refraction)
    simple_glass: Option<wgpu::RenderPipeline>,
}

/// Cached MSAA pipelines for dynamic sample counts
struct MsaaPipelines {
    /// Split SDF MSAA pipeline: core shapes
    sdf_core: wgpu::RenderPipeline,
    /// Split SDF MSAA pipeline: shadow shapes
    sdf_shadow: wgpu::RenderPipeline,
    /// Split SDF MSAA pipeline: 3D raymarched shapes
    sdf_3d: wgpu::RenderPipeline,
    /// Split SDF MSAA pipeline: notch shapes
    sdf_notch: wgpu::RenderPipeline,
    /// Path pipeline for this sample count
    path: wgpu::RenderPipeline,
    /// Sample count these pipelines were created for
    sample_count: u32,
}

/// GPU buffers for rendering
struct Buffers {
    /// Uniform buffer for viewport size
    uniforms: wgpu::Buffer,
    /// Storage buffer for SDF primitives
    primitives: wgpu::Buffer,
    /// Storage buffer for glass primitives
    glass_primitives: wgpu::Buffer,
    /// Uniform buffer for glass shader
    glass_uniforms: wgpu::Buffer,
    /// Storage buffer for text glyphs
    #[allow(dead_code)]
    glyphs: wgpu::Buffer,
    /// Uniform buffer for path rendering
    path_uniforms: wgpu::Buffer,
    /// Vertex buffer for path geometry (dynamic, recreated as needed)
    path_vertices: Option<wgpu::Buffer>,
    /// Index buffer for path geometry (dynamic, recreated as needed)
    path_indices: Option<wgpu::Buffer>,
    /// Pre-allocated uniform buffers for multi-pass blur (one per pass, max 8) — lazily created
    blur_uniforms_pool: Option<Vec<wgpu::Buffer>>,
    /// Cached uniform buffer for drop shadow effect — lazily created
    drop_shadow_uniforms: Option<wgpu::Buffer>,
    /// Cached uniform buffer for glow effect — lazily created
    glow_uniforms: Option<wgpu::Buffer>,
    /// Cached uniform buffer for color matrix effect — lazily created
    color_matrix_uniforms: Option<wgpu::Buffer>,
    /// Storage buffer for auxiliary per-primitive data (group shapes, polygon clips)
    aux_data: wgpu::Buffer,
    /// Instance vertex buffer for VERTEX_STORAGE fallback (WebGL2).
    /// Created/resized on demand when the adapter lacks storage buffers in
    /// vertex shaders.
    sdf_vertex_instances: Option<wgpu::Buffer>,
    /// Data texture for primitive data (WebGL2 fallback when no storage buffers).
    /// Width follows GpuPrimitive::DATA_TEXTURE_TEXELS; height is max_primitives.
    /// Format: Rgba32Float.
    prim_data_texture: Option<wgpu::Texture>,
    prim_data_view: Option<wgpu::TextureView>,
    /// Data texture for auxiliary data (WebGL2 fallback when no storage buffers).
    /// Width = 1024 texels, height grows on demand. Format: Rgba32Float.
    aux_data_texture: Option<wgpu::Texture>,
    aux_data_view: Option<wgpu::TextureView>,
    /// Current height of the aux data texture (for resize detection)
    aux_data_texture_height: u32,
    /// Data texture for glyph data (WebGL2 fallback).
    /// Width = 6 texels (one per vec4 field of GpuGlyph), height = max_glyphs.
    glyph_data_texture: Option<wgpu::Texture>,
    glyph_data_view: Option<wgpu::TextureView>,
}

/// Bind groups for shader resources
struct BindGroups {
    /// Bind group for SDF pipeline
    sdf: wgpu::BindGroup,
    /// Bind group for glass pipeline (needs backdrop texture)
    glass: Option<wgpu::BindGroup>,
    /// Bind group for path pipeline
    path: wgpu::BindGroup,
}

/// Cached glass resources to avoid per-frame allocations
struct CachedGlassResources {
    /// Sampler for backdrop texture (reused across frames)
    sampler: wgpu::Sampler,
    /// Cached bind group (valid when backdrop texture hasn't changed)
    bind_group: Option<wgpu::BindGroup>,
    data: Option<glass::GlassDataTexture>,
}

/// Active glyph atlas pointers for SDF bind group (set per-frame).
///
/// When CSS-transformed text is present, the real glyph atlas textures are bound
/// into `self.bind_groups.sdf` instead of the placeholder textures. These pointers
/// track the currently-bound atlas views so that `rebind_sdf_bind_group()` (called
/// during aux buffer resize) can recreate the bind group with the real atlas.
///
/// SAFETY: Pointers are valid for the duration of a frame — they point to TextureViews
/// owned by the text context, which outlives all render calls within a frame.
struct ActiveGlyphAtlas {
    atlas_view_ptr: *const wgpu::TextureView,
    color_atlas_view_ptr: *const wgpu::TextureView,
}

/// Cached resources for SDF 3D raymarching viewports
struct Sdf3DResources {
    /// Bind group layout for SDF 3D uniforms
    bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform buffer for SDF 3D uniforms
    uniform_buffer: wgpu::Buffer,
    /// Bind group for SDF 3D uniforms
    bind_group: wgpu::BindGroup,
    /// Cached pipelines keyed by shader hash
    pipeline_cache: HashMap<u64, wgpu::RenderPipeline>,
}

/// Primitive range boundaries for split SDF pipeline dispatch.
///
/// After sorting primitives by `SdfPipelineCategory`, each category
/// occupies a contiguous range in the GPU buffer. Text primitives are
/// tracked here for completeness but rendered by the separate text pipeline.
#[derive(Clone, Default)]
struct SdfPrimitiveRanges {
    core: std::ops::Range<u32>,
    shadow: std::ops::Range<u32>,
    sdf_3d: std::ops::Range<u32>,
    notch: std::ops::Range<u32>,
    text: std::ops::Range<u32>,
    /// Contiguous `(category, start, end)` runs covering the primitive
    /// stream in instance-index order. `draw_split_sdf` issues one draw
    /// per run so cross-category z-order is preserved (the split pipelines
    /// otherwise lose it when each pipeline runs over the full range).
    runs: Vec<(SdfPipelineCategory, u32, u32)>,
}

/// The GPU renderer using wgpu
///
/// This is the main rendering engine that:
/// - Manages wgpu device, queue, and surface
/// - Creates and manages render pipelines for different primitive types
/// - Batches primitives for efficient GPU rendering
/// - Executes render passes
pub struct GpuRenderer {
    /// wgpu instance
    #[allow(dead_code)]
    instance: wgpu::Instance,
    /// GPU adapter
    #[allow(dead_code)]
    adapter: wgpu::Adapter,
    /// GPU device
    pub(crate) device: Arc<wgpu::Device>,
    /// Command queue
    pub(crate) queue: Arc<wgpu::Queue>,
    /// Render pipelines
    pipelines: Pipelines,
    /// Effect pipelines (lazily created on first use)
    effect_pipelines: EffectPipelines,
    /// Cached MSAA pipelines for overlay rendering
    msaa_pipelines: Option<MsaaPipelines>,
    /// GPU buffers
    buffers: Buffers,
    /// Bind groups
    bind_groups: BindGroups,
    /// Bind group layouts
    bind_group_layouts: BindGroupLayouts,
    /// Current viewport size
    pub(crate) viewport_size: (u32, u32),
    /// Saved viewport size during offscreen rendering (for restore_viewport)
    saved_viewport_size: Option<(u32, u32)>,
    /// Optional scissor rect applied to subsequent text/image render
    /// passes. Set via `set_pending_scissor` / cleared via
    /// `clear_pending_scissor`. Used by the compositor v2 damage-rect
    /// path to confine text / SVG / image dispatches to the same
    /// scissor region as the SDF clear-and-redraw run by
    /// `render_static_layer_damaged`. `None` = no scissor (default;
    /// full-attachment dispatch).
    pending_scissor: Option<(u32, u32, u32, u32)>,
    /// Damage scissor applied to subsequent layer-composite blits
    /// inside `blit_tight_texture_to_target`. Unlike
    /// [`Self::pending_scissor`] (a hard replacement used by text /
    /// SVG / image overlays), this scissor is *intersected* with
    /// the blit's own visible-bounds scissor so layer composites
    /// can only paint inside the union of damage rect AND the
    /// layer's content area. Phase 4d Opt 2 sets this on the
    /// damage path so re-rendering a scissored cache region keeps
    /// effect-layer composites confined to the damage rect.
    /// `None` = no damage scissor (default; blit uses its own
    /// visible-bounds scissor only).
    pending_damage_scissor: Option<(u32, u32, u32, u32)>,
    /// Renderer configuration
    config: RendererConfig,
    /// Current frame time (for animations)
    time: f32,
    /// Resolved texture format used by pipelines
    pub(crate) texture_format: wgpu::TextureFormat,
    /// Lazily-created image pipeline and resources
    image_pipeline: Option<ImagePipeline>,
    /// Lazily-created mesh rendering pipeline
    pub(crate) mesh_pipeline: Option<MeshPipeline>,
    /// User-registered custom render passes
    custom_passes: crate::custom_pass::CustomPassManager,
    /// GPU texture memory budget and tracking
    memory_budget: GpuMemoryBudget,
    /// Cached MSAA textures for overlay rendering (avoids per-frame allocation)
    overlay_targets: overlay_targets::OverlayTargets,
    overlay_alpha: Option<overlay::AlphaPass>,
    /// Cached glass resources (avoids per-frame allocation)
    cached_glass: Option<CachedGlassResources>,
    /// Placeholder glyph atlas texture view (1x1 transparent) for SDF bind group
    placeholder_glyph_atlas_view: wgpu::TextureView,
    /// Placeholder color glyph atlas texture view (1x1 transparent) for SDF bind group
    placeholder_color_glyph_atlas_view: wgpu::TextureView,
    /// Sampler for glyph atlas textures
    glyph_sampler: wgpu::Sampler,
    /// Active glyph atlas pointers — when set, `self.bind_groups.sdf` uses real atlas
    active_glyph_atlas: Option<ActiveGlyphAtlas>,
    /// Gradient texture cache for multi-stop gradient support on paths
    gradient_texture_cache: GradientTextureCache,
    /// Placeholder image texture (1x1 white) for path bind group when no image is used
    placeholder_path_image_view: wgpu::TextureView,
    /// Sampler for path image textures
    path_image_sampler: wgpu::Sampler,
    /// Layer texture cache for offscreen rendering and composition
    layer_texture_cache: LayerTextureCache,
    /// Cached resources for SDF 3D raymarching viewports (lazily initialized)
    sdf_3d_resources: Option<Sdf3DResources>,
    /// Cached particle systems for GPU particle rendering (keyed by hash of emitter config)
    particle_systems: std::collections::HashMap<u64, crate::particles::ParticleSystemGpu>,
    /// Cache of loaded mask images by URL/path
    mask_image_cache: HashMap<String, crate::image::GpuImage>,
    /// Dummy 1x1 texture view for blend mode dest binding when not needed (Normal mode)
    dummy_blend_dest_view: wgpu::TextureView,
    /// Dummy 1x1 texture for blend mode dest (needed for copy_texture_to_texture)
    dummy_blend_dest_texture: wgpu::Texture,
    /// Current render target texture pointer for blend mode two-pass compositing.
    /// Set via `set_blend_target()` before rendering, cleared after.
    /// Safety: Only valid during an active render frame.
    blend_target_ptr: Option<*const wgpu::Texture>,
    /// Cached @flow GPU pipelines (built lazily from a flow's WGSL)
    flow_pipeline_cache: crate::flow_pipeline::FlowPipelineCache,
    /// The window cursor in physical pixels: the `pointer` builtin of a flow that is not given one.
    flow_cursor: [f32; 2],
    /// Staging texture for scene capture (used by flow shaders with sample_scene()).
    /// Lazily created/resized to match the render target.
    scene_copy_texture: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
    /// The frame that last used `scene_copy_texture`.
    scene_copy_last_frame: u64,
    /// The frame that last drew meshes into the mesh pass's window-sized targets.
    pub(crate) mesh_targets_last_frame: u64,
    /// Whether the GPU adapter supports storage buffers in vertex shaders.
    /// When `false`, SDF pipelines use an instance-stepped vertex buffer
    /// fallback (WebGL2 path).
    has_vertex_storage: bool,
    /// Whether the GPU adapter supports storage buffers at all
    /// (i.e. `max_storage_buffers_per_shader_stage > 0`).
    /// When `false`, the renderer uses data textures (Rgba32Float) to pass
    /// primitive and auxiliary data to fragment shaders instead of storage
    /// buffers. This is the Tier 3 / WebGL2 fallback path.
    pub(crate) has_storage_buffers: bool,
    /// `true` when `device.features()` reports
    /// `TEXTURE_COMPRESSION_BC` — the renderer can upload BC1 / BC3
    /// / BC4 / BC5 material textures and cut GPU VRAM by 4-8× vs
    /// Rgba8. When `false`, upload paths must fall back to Rgba8
    /// (all desktop adapters support BC; WebGL2 on iOS Safari and
    /// a handful of older browsers do not).
    pub(crate) has_texture_compression_bc: bool,
    /// Layer-compositor static cache. Holds the most recent full-paint
    /// output for the bg primitive batch / text / SVG / image / flow
    /// passes, *excluding* canvas content (the walker is invoked with
    /// `RenderTree::set_skip_canvas_drawing(true)` for that pass).
    /// Each compositor frame blits this texture to the surface and
    /// then draws fresh canvas content on top — see
    /// `blit_static_layer_to` and `render_overlay` for the dispatch
    /// side. Lazily allocated; reset on viewport resize.
    static_layer: Option<StaticLayer>,
}

/// Offscreen render target backing the layer compositor's static-tree
/// cache. The renderer renders the full bg / text / SVG / image / flow
/// content into this texture once per "static" full paint; every
/// subsequent fast-path frame copies the texture onto the surface
/// and overlays only the canvas primitives that change frame-to-frame.
pub struct StaticLayer {
    /// Storage for the cached image. Sized to the current viewport;
    /// reallocated on resize.
    pub(crate) texture: wgpu::Texture,
    /// View used as a render attachment when rendering INTO the cache
    /// (`RenderAttachment` usage) and bound for the blit when reading
    /// FROM it (`COPY_SRC` usage).
    pub(crate) view: wgpu::TextureView,
    /// Physical pixel width.
    pub(crate) width: u32,
    /// Physical pixel height.
    pub(crate) height: u32,
    /// `true` after the renderer has written a frame into the texture;
    /// `false` after construction or invalidation. The compositor
    /// path falls back to full paint while `valid == false`.
    pub(crate) valid: bool,
}

/// Image rendering pipeline (created lazily on first image render)
struct ImagePipeline {
    pipeline: wgpu::RenderPipeline,
    replacement: Option<[wgpu::RenderPipeline; 2]>,
    bind_group_layout: wgpu::BindGroupLayout,
    instance_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
}

/// Shadow map resolution (square)
pub(crate) const SHADOW_MAP_SIZE: u32 = 2048;

use crate::mesh_pipeline::{
    MAX_MORPH_TARGETS, MESH_CACHE_CAPACITY, MORPH_CACHE_CAPACITY, MeshBufferCacheEntry,
    MeshPipeline,
};

struct BindGroupLayouts {
    sdf: wgpu::BindGroupLayout,
    glass: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    text: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    composite: wgpu::BindGroupLayout,
    path: wgpu::BindGroupLayout,
    /// Layout for layer composition shader
    layer_composite: wgpu::BindGroupLayout,
    /// Layout for blur effect shader
    blur: wgpu::BindGroupLayout,
    /// Layout for color matrix effect shader
    color_matrix: wgpu::BindGroupLayout,
    /// Layout for drop shadow effect shader
    drop_shadow: wgpu::BindGroupLayout,
    /// Layout for glow effect shader
    glow: wgpu::BindGroupLayout,
    /// Layout for mask image effect shader
    mask_image: wgpu::BindGroupLayout,
}

impl GpuRenderer {
    /// Whether the GPU adapter supports storage buffers.
    pub fn has_storage_buffers(&self) -> bool {
        self.has_storage_buffers
    }

    /// Whether the GPU device has `TEXTURE_COMPRESSION_BC` enabled.
    /// Callers that upload material textures should query this to
    /// decide between BC (when `true`) and Rgba8 fallback paths.
    pub fn has_texture_compression_bc(&self) -> bool {
        self.has_texture_compression_bc
    }

    fn create_renderer(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        texture_format: wgpu::TextureFormat,
        mut config: RendererConfig,
        viewport_size: (u32, u32),
    ) -> Result<Self, RendererError> {
        // Check if the adapter supports storage buffers in vertex shaders
        let has_vertex_storage = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::VERTEX_STORAGE);
        if !has_vertex_storage {
            tracing::info!("VERTEX_STORAGE not supported — using instance vertex buffer fallback");
        }

        // Check if the device enabled BC texture compression (requested
        // via `requested_device_features` earlier; we check the
        // *device* features, not the adapter's, because the device
        // may have been opened with a subset).
        let has_texture_compression_bc = device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        if has_texture_compression_bc {
            tracing::info!(
                "BC texture compression enabled — material textures upload as BC1/3/4/5"
            );
        } else {
            tracing::info!(
                "BC texture compression unavailable on this adapter — material textures upload as Rgba8"
            );
        }

        // Check if the adapter supports storage buffers at all (Tier 3 / DT fallback)
        let has_storage_buffers = adapter.limits().max_storage_buffers_per_shader_stage > 0;
        if !has_storage_buffers {
            tracing::info!("No storage buffer support — using data texture fallback (WebGL2 mode)");
            // When there are no storage buffers, max_storage_buffer_binding_size is 0,
            // so apply_renderer_config_overrides clamped max_primitives/max_glyphs to 1.
            // Re-apply sensible defaults clamped by texture dimension limits instead.
            let tex_max = adapter.limits().max_texture_dimension_2d as usize;
            let defaults = RendererConfig::default();
            // Use env overrides if present, otherwise fall back to defaults
            config.max_primitives = env_usize("BLINC_GPU_MAX_PRIMITIVES")
                .unwrap_or(defaults.max_primitives)
                .clamp(1, tex_max);
            config.max_glyphs = env_usize("BLINC_GPU_MAX_GLYPHS")
                .unwrap_or(defaults.max_glyphs)
                .clamp(1, tex_max);
            log_renderer_config(&config);
        }

        // Create bind group layouts
        let bind_group_layouts = Self::create_bind_group_layouts_with_flags(
            &device,
            has_vertex_storage,
            has_storage_buffers,
        );

        // Create shaders

        let text_source = if has_storage_buffers {
            TEXT_SHADER
        } else {
            TEXT_DT_SHADER
        };
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text Shader"),
            source: wgpu::ShaderSource::Wgsl(text_source.into()),
        });

        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
        });

        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Path Shader"),
            source: wgpu::ShaderSource::Wgsl(
                if has_storage_buffers {
                    PATH_SHADER
                } else {
                    PATH_DT_SHADER
                }
                .into(),
            ),
        });

        let layer_composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Layer Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(LAYER_COMPOSITE_SHADER.into()),
        });

        let clear_quad_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Clear Quad Shader"),
            source: wgpu::ShaderSource::Wgsl(CLEAR_QUAD_SHADER.into()),
        });

        // Split SDF shaders — specialized pipelines for each primitive category.
        // Three tiers:
        //   Tier 1 (full): Storage buffers in VS + FS — sdf_*.wgsl
        //   Tier 2 (VB):   No VS storage, FS storage works — sdf_*_vb.wgsl + vertex buffer
        //   Tier 3 (DT):   No storage at all (WebGL2) — sdf_*_dt.wgsl + VB + data textures
        let (sdf_core_source, sdf_shadow_source, sdf_3d_source, sdf_notch_source) =
            if !has_storage_buffers {
                // Tier 3: Data texture fallback (no storage buffers at all)
                (
                    SDF_CORE_DT_SHADER,
                    SDF_SHADOW_DT_SHADER,
                    SDF_3D_DT_SHADER,
                    SDF_NOTCH_DT_SHADER,
                )
            } else if !has_vertex_storage {
                // Tier 2: VB fallback (no VS storage, FS storage works)
                (
                    SDF_CORE_VB_SHADER,
                    SDF_SHADOW_VB_SHADER,
                    SDF_3D_VB_SHADER,
                    SDF_NOTCH_VB_SHADER,
                )
            } else {
                // Tier 1: Full storage buffer support
                (
                    SDF_CORE_SHADER,
                    SDF_SHADOW_SHADER,
                    SDF_3D_SHADER,
                    SDF_NOTCH_SHADER,
                )
            };

        let sdf_core_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Core Shader"),
            source: wgpu::ShaderSource::Wgsl(sdf_core_source.into()),
        });
        // `sdf_core` with the optional blocks off, specialised when the crate is built. Like the
        // compact shader below it reads the primitive in the vertex stage: full tier only.
        let sdf_core_simple_shader = has_vertex_storage.then(|| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("SDF Core Simple Shader"),
                source: wgpu::ShaderSource::Wgsl(crate::sdf_russl::simple::WGSL.into()),
            })
        });
        // The compact shader reads the primitive in the vertex stage, so it needs the full tier.
        let sdf_simple_shader = has_vertex_storage.then(|| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("SDF Simple Shader"),
                source: wgpu::ShaderSource::Wgsl(crate::shaders::SDF_SIMPLE_SHADER.into()),
            })
        });
        let sdf_shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Shadow Shader"),
            source: wgpu::ShaderSource::Wgsl(sdf_shadow_source.into()),
        });
        let sdf_3d_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF 3D Shader"),
            source: wgpu::ShaderSource::Wgsl(sdf_3d_source.into()),
        });
        let sdf_notch_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Notch Shader"),
            source: wgpu::ShaderSource::Wgsl(sdf_notch_source.into()),
        });

        // Persisted pipeline cache: seed from disk (validated against this
        // adapter), feed it to pipeline creation, then write it back. Skips
        // the bulk of pipeline (re)build on every launch after the first.
        let adapter_info = adapter.get_info();
        let pipeline_cache = if device.features().contains(wgpu::Features::PIPELINE_CACHE) {
            let data = config
                .pipeline_cache_path
                .as_deref()
                .and_then(|p| load_pipeline_cache_data(p, &adapter_info));
            tracing::info!(
                "Pipeline cache: {} ({} bytes)",
                if data.is_some() { "loaded" } else { "cold" },
                data.as_deref().map(<[u8]>::len).unwrap_or(0),
            );
            // SAFETY: `data`, when Some, is bytes we previously produced via
            // `PipelineCache::get_data` and validated against this exact
            // adapter in `load_pipeline_cache_data`. `fallback: true` also
            // makes the driver ignore data it can't use.
            Some(unsafe {
                device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                    label: Some("Blinc Pipeline Cache"),
                    data: data.as_deref(),
                    fallback: true,
                })
            })
        } else {
            None
        };

        // Create pipelines (core only — effect pipelines are lazy)
        let pipelines = Self::create_pipelines(
            &device,
            &bind_group_layouts,
            &sdf_core_shader,
            sdf_core_simple_shader.as_ref(),
            sdf_simple_shader.as_ref(),
            &sdf_shadow_shader,
            &sdf_3d_shader,
            &sdf_notch_shader,
            &text_shader,
            &composite_shader,
            &path_shader,
            &layer_composite_shader,
            &clear_quad_shader,
            texture_format,
            config.sample_count,
            has_vertex_storage,
            pipeline_cache.as_ref(),
        );

        // Persist the (now-populated) cache for the next launch. Best-effort.
        if let (Some(cache), Some(path)) = (
            pipeline_cache.as_ref(),
            config.pipeline_cache_path.as_deref(),
        ) && let Some(data) = cache.get_data()
        {
            tracing::info!("Pipeline cache: saving {} bytes", data.len());
            save_pipeline_cache_data(path, &adapter_info, &data);
        }

        // Create buffers (storage buffers always created; DT textures added when needed)
        let buffers = Self::create_buffers(&device, &config, has_storage_buffers);

        // Create placeholder glyph atlas textures (1x1 transparent)
        // These are used when no text is rendered, satisfying the bind group layout
        let placeholder_glyph_atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder Glyph Atlas"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm, // Grayscale for regular glyphs
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let placeholder_glyph_atlas_view =
            placeholder_glyph_atlas.create_view(&wgpu::TextureViewDescriptor::default());

        let placeholder_color_glyph_atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder Color Glyph Atlas"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb, // RGBA for color emoji
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let placeholder_color_glyph_atlas_view =
            placeholder_color_glyph_atlas.create_view(&wgpu::TextureViewDescriptor::default());

        // Create sampler for glyph atlases
        let glyph_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Glyph Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create gradient texture cache for multi-stop gradients on paths
        let gradient_texture_cache = GradientTextureCache::new(&device, &queue);

        // Create placeholder image texture for paths (1x1 white)
        let placeholder_path_image = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder Path Image"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // Initialize with white pixel
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &placeholder_path_image,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255u8, 255, 255, 255], // White pixel
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let placeholder_path_image_view =
            placeholder_path_image.create_view(&wgpu::TextureViewDescriptor::default());

        // Create sampler for path image textures
        let path_image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Path Image Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create 1x1 dummy texture for blend mode dest binding (Normal mode doesn't read it)
        let dummy_blend_dest = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Dummy Blend Dest"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &dummy_blend_dest,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0u8, 0, 0, 0], // Transparent pixel
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let dummy_blend_dest_view =
            dummy_blend_dest.create_view(&wgpu::TextureViewDescriptor::default());

        // Create initial bind groups
        let bind_groups = Self::create_bind_groups(
            &device,
            &bind_group_layouts,
            &buffers,
            &placeholder_glyph_atlas_view,
            &placeholder_color_glyph_atlas_view,
            &glyph_sampler,
            &gradient_texture_cache,
            &placeholder_path_image_view,
            &path_image_sampler,
            has_storage_buffers,
        );

        let flow_pipeline_cache =
            crate::flow_pipeline::FlowPipelineCache::new(device.clone(), texture_format);

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            pipelines,
            effect_pipelines: EffectPipelines {
                blur: None,
                color_matrix: None,
                drop_shadow: None,
                glow: None,
                mask_image: None,
                glass: None,
                simple_glass: None,
            },
            msaa_pipelines: None,
            buffers,
            bind_groups,
            bind_group_layouts,
            viewport_size,
            saved_viewport_size: None,
            pending_scissor: None,
            pending_damage_scissor: None,
            memory_budget: GpuMemoryBudget::new(config.gpu_memory_budget),
            config,
            time: 0.0,
            texture_format,
            image_pipeline: None,
            mesh_pipeline: None,
            custom_passes: crate::custom_pass::CustomPassManager::new(),
            overlay_targets: overlay_targets::OverlayTargets::default(),
            overlay_alpha: None,
            cached_glass: None,
            placeholder_glyph_atlas_view,
            placeholder_color_glyph_atlas_view,
            glyph_sampler,
            active_glyph_atlas: None,
            gradient_texture_cache,
            placeholder_path_image_view,
            path_image_sampler,
            layer_texture_cache: LayerTextureCache::new(texture_format),
            sdf_3d_resources: None,
            particle_systems: std::collections::HashMap::new(),
            mask_image_cache: HashMap::new(),
            dummy_blend_dest_view,
            dummy_blend_dest_texture: dummy_blend_dest,
            blend_target_ptr: None,
            flow_pipeline_cache,
            flow_cursor: [0.0; 2],
            scene_copy_texture: None,
            scene_copy_last_frame: 0,
            mesh_targets_last_frame: 0,
            has_vertex_storage,
            has_storage_buffers,
            has_texture_compression_bc,
            static_layer: None,
        })
    }

    /// Resize the viewport
    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport_size = (width, height);
    }

    /// Current viewport dimensions in physical pixels — width, height.
    /// Used by the compositor v2 damage-rect path on the app side to
    /// clamp scissor rects to the static-layer extent before staging.
    pub fn viewport_size(&self) -> (u32, u32) {
        self.viewport_size
    }

    /// Stage a scissor rect that subsequent text / image dispatches
    /// will apply via `set_scissor_rect`. Used by the compositor v2
    /// damage-rect path to confine text/SVG/image re-dispatches to
    /// the same region the SDF clear + redraw covered. Stays in
    /// effect until [`Self::clear_pending_scissor`] is called.
    pub fn set_pending_scissor(&mut self, rect: (u32, u32, u32, u32)) {
        self.pending_scissor = Some(rect);
    }

    /// Damage-scissor variant of [`Self::set_pending_scissor`]: the
    /// rect is *intersected* with `blit_tight_texture_to_target`'s
    /// own visible-bounds scissor instead of replacing it. Phase 4d
    /// Opt 2 sets this before re-rendering a CSS-animated cache
    /// region so layer-effect composites paint only inside the
    /// damage rect, leaving the rest of the cache from last paint
    /// untouched. Call [`Self::clear_pending_damage_scissor`] when
    /// done.
    pub fn set_pending_damage_scissor(&mut self, rect: (u32, u32, u32, u32)) {
        self.pending_damage_scissor = Some(rect);
    }

    /// Clear the damage scissor — subsequent layer composites use
    /// their own visible-bounds scissor only.
    pub fn clear_pending_damage_scissor(&mut self) {
        self.pending_damage_scissor = None;
    }

    /// Drop the staged scissor so subsequent dispatches paint to the
    /// full attachment again. Paired with `set_pending_scissor`.
    pub fn clear_pending_scissor(&mut self) {
        self.pending_scissor = None;
    }

    /// Set the current render target texture for blend mode two-pass compositing.
    ///
    /// Must be called before `render_overlay()` when the batch may contain
    /// non-Normal blend modes. The texture must remain valid until
    /// `clear_blend_target()` is called.
    ///
    /// # Safety contract
    /// The caller guarantees the texture reference outlives the render frame.
    /// The pointer is only dereferenced within `blit_texture_to_target`.
    pub fn set_blend_target(&mut self, texture: &wgpu::Texture) {
        self.blend_target_ptr = Some(texture as *const wgpu::Texture);
    }

    /// Clear the blend target texture reference after rendering.
    pub fn clear_blend_target(&mut self) {
        self.blend_target_ptr = None;
    }

    /// Update the frame time (for animations)
    pub fn update_time(&mut self, time: f32) {
        self.time = time;
    }

    /// Get the wgpu device
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Get the wgpu device as Arc
    pub fn device_arc(&self) -> Arc<wgpu::Device> {
        self.device.clone()
    }

    /// Get the wgpu queue
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Get the wgpu queue as Arc
    pub fn queue_arc(&self) -> Arc<wgpu::Queue> {
        self.queue.clone()
    }

    /// Surface texture format the renderer was configured with. Used by
    /// the custom-pass dispatch site so user passes know what format
    /// their `initialize` should target.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.texture_format
    }

    /// Create a surface from platform raw handles using the renderer's
    /// existing instance.
    ///
    /// This is used by platforms such as Android where the OS can destroy
    /// and recreate the native window while the GPU device and render tree
    /// should remain alive.
    ///
    /// # Safety
    ///
    /// The caller must uphold [`wgpu::Instance::create_surface_unsafe`]'s
    /// handle validity and lifetime requirements.
    pub unsafe fn create_surface_unsafe(
        &self,
        target: wgpu::SurfaceTargetUnsafe,
    ) -> Result<wgpu::Surface<'static>, RendererError> {
        unsafe { self.instance.create_surface_unsafe(target) }.map_err(RendererError::SurfaceError)
    }

    /// The adapter the device/queue were created against. Needed for
    /// `Surface::get_capabilities` so callers can negotiate format /
    /// alpha mode / present mode against what the OS compositor
    /// actually exposes.
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    /// Get the texture format used by this renderer's pipelines
    pub fn texture_format(&self) -> wgpu::TextureFormat {
        self.texture_format
    }

    /// Returns true if unified text/SDF rendering is enabled
    ///
    /// When enabled, text glyphs are converted to SDF primitives and rendered
    /// in the same GPU pass as other shapes, ensuring consistent transforms
    /// during animations.
    pub fn unified_text_rendering(&self) -> bool {
        self.config.unified_text_rendering
    }

    /// Poll the device to process completed GPU operations and free resources.
    /// Call this after frame rendering to prevent memory accumulation.
    pub fn poll(&self) {
        // wgpu 26: `Maintain::Wait` was renamed to `PollType::Wait`. Result
        // is a `Result<PollStatus, _>` rather than the old `MaintainResult`,
        // and we don't care about the precise status here — we just want
        // to block until the GPU is idle.
        //
        // Mobile skips the wait: the swapchain paces frames, and idling the CPU until the
        // GPU finishes serialises encode with execution.
        let poll = if cfg!(any(target_os = "ios", target_os = "android")) {
            wgpu::PollType::Poll
        } else {
            wgpu::PollType::Wait
        };
        let _ = self.device.poll(poll);
    }

    /// Bind real glyph atlas textures into the default SDF bind group.
    ///
    /// Call once per frame before any rendering when CSS-transformed text is present.
    /// This replaces the placeholder atlas with the real glyph atlas in
    /// `self.bind_groups.sdf`, so ALL render paths automatically get the atlas
    /// without needing to thread it through every method.
    ///
    /// Always rebuilds the bind group. The previous pointer-equality
    /// optimisation was a false-negative trap: `text_ctx` replaces
    /// the `atlas_view` field *in place* (same `Option<TextureView>`
    /// memory address, different inner `Arc`) when the atlas grows.
    /// The borrowed reference `atlas_view as *const TextureView`
    /// returned the same pointer before and after growth, so the
    /// check thought "no change" while the underlying view was a
    /// different texture — the bind group kept the OLD view's Arc
    /// and subsequent glyph samples landed in the old atlas.
    ///
    /// Symptom: canvas_demo `ctx.draw_text` rendered blank because
    /// `collect_canvas_overlay` grew the atlas after the slow
    /// path's `set_glyph_atlas` call, the bind group stayed on the
    /// pre-growth view, and the new glyph UVs (referring to the
    /// post-growth atlas) sampled from the wrong texture.
    ///
    /// Cost of always rebuilding: one `create_bind_group` call
    /// (~50 µs) per `set_glyph_atlas` invocation. Per frame on the
    /// slow path that's negligible; the overlay-rebind helper
    /// `BlincApp::rebind_glyph_atlas_for_overlay` calls this once
    /// per `composite_frame` site, so total cost is one extra
    /// rebuild per frame at most.
    ///
    /// SAFETY: The raw pointers stored in `active_glyph_atlas` must
    /// remain valid for the duration of the frame — guaranteed
    /// because they point to TextureViews owned by the text
    /// context, which outlives all render calls.
    pub fn set_glyph_atlas(
        &mut self,
        atlas_view: &wgpu::TextureView,
        color_atlas_view: &wgpu::TextureView,
    ) {
        let atlas_ptr = atlas_view as *const wgpu::TextureView;
        let color_ptr = color_atlas_view as *const wgpu::TextureView;
        self.active_glyph_atlas = Some(ActiveGlyphAtlas {
            atlas_view_ptr: atlas_ptr,
            color_atlas_view_ptr: color_ptr,
        });
        self.rebind_sdf_bind_group();
    }

    /// Register a custom render pass.
    ///
    /// The pass will be initialized immediately and executed each frame
    /// at the stage returned by `pass.stage()`.
    pub fn register_custom_pass(
        &mut self,
        mut pass: Box<dyn crate::custom_pass::CustomRenderPass>,
    ) {
        pass.initialize(&self.device, &self.queue, self.texture_format);
        self.custom_passes.register(pass);
    }

    /// Remove a custom render pass by label.
    pub fn remove_custom_pass(&mut self, label: &str) -> bool {
        self.custom_passes.remove(label)
    }

    /// Execute all custom passes for a given stage.
    pub fn execute_custom_passes(
        &mut self,
        stage: crate::custom_pass::RenderStage,
        target: &wgpu::TextureView,
        scale_factor: f64,
    ) {
        if !self.custom_passes.has_passes(stage) {
            return;
        }
        let ctx = crate::custom_pass::RenderPassContext {
            device: &self.device,
            queue: &self.queue,
            target,
            viewport_width: self.viewport_size.0,
            viewport_height: self.viewport_size.1,
            texture_format: self.texture_format,
            scale_factor,
            view_proj: None,
            inv_view_proj: None,
            camera_pos: None,
            viewport: None,
        };
        self.custom_passes.execute_stage(stage, &ctx);
    }

    /// Execute Scene3D custom passes with camera context.
    pub fn execute_scene3d_passes(
        &mut self,
        target: &wgpu::TextureView,
        scale_factor: f64,
        view_proj: &[f32; 16],
        inv_view_proj: &[f32; 16],
        camera_pos: [f32; 3],
        viewport: Option<[f32; 4]>,
    ) {
        let stage = crate::custom_pass::RenderStage::Scene3D;
        if !self.custom_passes.has_passes(stage) {
            return;
        }
        let ctx = crate::custom_pass::RenderPassContext {
            device: &self.device,
            queue: &self.queue,
            target,
            viewport_width: self.viewport_size.0,
            viewport_height: self.viewport_size.1,
            texture_format: self.texture_format,
            scale_factor,
            view_proj: Some(*view_proj),
            inv_view_proj: Some(*inv_view_proj),
            camera_pos: Some(camera_pos),
            viewport,
        };
        self.custom_passes.execute_stage(stage, &ctx);
    }

    /// Notify custom passes of a viewport resize.
    pub fn resize_custom_passes(&mut self, width: u32, height: u32) {
        self.custom_passes.resize(&self.device, width, height);
    }

    // ─── GPU Memory Budget ─────────────────────────────────────────────────

    /// Enforce the GPU memory budget by evicting cached textures.
    ///
    /// Call once per frame (e.g., at frame start) to keep memory in check.
    /// Evicts largest pooled textures first, then trims mask image cache
    /// if still over budget.
    pub fn enforce_memory_budget(&mut self) {
        self.memory_budget.reset_transient();

        if self.memory_budget.budget() == 0 {
            return; // unlimited
        }

        let layer_bytes = self.layer_texture_cache.stats().total_memory_bytes();
        if !self.memory_budget.is_over_budget(layer_bytes) {
            return;
        }

        // Phase 1: evict pooled layer textures (largest first)
        let target = self
            .memory_budget
            .budget()
            .saturating_sub(self.memory_budget.mask_image_bytes);
        let freed = self.layer_texture_cache.evict_to_budget(target);
        if freed > 0 {
            self.memory_budget.record_eviction();
        }

        // Phase 2: if still over, trim mask image cache (drop oldest entries)
        let layer_bytes = self.layer_texture_cache.stats().total_memory_bytes();
        if self.memory_budget.is_over_budget(layer_bytes) && !self.mask_image_cache.is_empty() {
            // Remove one entry at a time until under budget
            let keys: Vec<String> = self.mask_image_cache.keys().cloned().collect();
            for key in keys {
                if !self
                    .memory_budget
                    .is_over_budget(self.layer_texture_cache.stats().total_memory_bytes())
                {
                    break;
                }
                if let Some(img) = self.mask_image_cache.remove(&key) {
                    let (w, h) = img.dimensions();
                    self.memory_budget.untrack_mask_image(w, h);
                    self.memory_budget.record_eviction();
                }
            }
        }
    }

    /// Get the current GPU memory budget tracker.
    pub fn memory_budget(&self) -> &GpuMemoryBudget {
        &self.memory_budget
    }

    /// Get estimated total GPU texture memory usage in bytes.
    pub fn estimated_texture_memory(&self) -> u64 {
        let layer_bytes = self.layer_texture_cache.stats().total_memory_bytes();
        self.memory_budget.total_tracked_bytes(layer_bytes)
    }

    /// Blit a tight texture to the target at the correct position
    #[allow(clippy::too_many_arguments)]
    pub fn blit_tight_texture_to_target(
        &mut self,
        source: &wgpu::TextureView,
        source_size: (u32, u32),
        target: &wgpu::TextureView,
        dest_pos: (f32, f32),
        dest_size: (f32, f32),
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
        clip: Option<([f32; 4], [f32; 4])>, // (clip_bounds, clip_radius)
        transform_3d: Option<blinc_core::Transform3DParams>,
        rotate_z_rad: f32,
    ) {
        use crate::primitives::LayerCompositeUniforms;

        let vp_w = self.viewport_size.0 as f32;
        let vp_h = self.viewport_size.1 as f32;

        // For 3D perspective transforms, compute the expanded bounding box of the
        // perspective-distorted quad corners so the scissor rect is large enough.
        let (effective_dest_pos, effective_dest_size) = if let Some(ref t3d) = transform_3d {
            let cx = dest_pos.0 + dest_size.0 * 0.5;
            let cy = dest_pos.1 + dest_size.1 * 0.5;
            let hw = dest_size.0 * 0.5;
            let hh = dest_size.1 * 0.5;
            // Project all 4 corners through perspective and find AABB
            let corners = [(-hw, -hh), (hw, -hh), (-hw, hh), (hw, hh)];
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;
            for (lx, ly) in corners {
                // Rotate Y
                let ry_x = lx * t3d.cos_ry;
                let ry_z = lx * t3d.sin_ry;
                // Rotate X
                let rx_y = ly * t3d.cos_rx - ry_z * t3d.sin_rx;
                let rx_z = ly * t3d.sin_rx + ry_z * t3d.cos_rx;
                // Perspective
                let w = (t3d.perspective_d + rx_z) / t3d.perspective_d;
                let sx = cx + ry_x / w;
                let sy = cy + rx_y / w;
                min_x = min_x.min(sx);
                min_y = min_y.min(sy);
                max_x = max_x.max(sx);
                max_y = max_y.max(sy);
            }
            ((min_x, min_y), (max_x - min_x, max_y - min_y))
        } else {
            (dest_pos, dest_size)
        };

        // Calculate the visible region by intersecting dest rect with viewport and clip bounds
        // Start with destination rect (possibly expanded for 3D)
        let mut vis_x0 = effective_dest_pos.0;
        let mut vis_y0 = effective_dest_pos.1;
        let mut vis_x1 = effective_dest_pos.0 + effective_dest_size.0;
        let mut vis_y1 = effective_dest_pos.1 + effective_dest_size.1;

        // Intersect with viewport
        vis_x0 = vis_x0.max(0.0);
        vis_y0 = vis_y0.max(0.0);
        vis_x1 = vis_x1.min(vp_w);
        vis_y1 = vis_y1.min(vp_h);

        // Intersect with clip bounds if provided
        let (clip_bounds, clip_radius, clip_type) = match clip {
            Some((bounds, radius)) => {
                // Intersect with clip bounds
                vis_x0 = vis_x0.max(bounds[0]);
                vis_y0 = vis_y0.max(bounds[1]);
                vis_x1 = vis_x1.min(bounds[0] + bounds[2]);
                vis_y1 = vis_y1.min(bounds[1] + bounds[3]);
                (bounds, radius, 1)
            }
            None => ([0.0, 0.0, vp_w, vp_h], [0.0; 4], 0),
        };

        // Check if anything is visible
        let vis_w = vis_x1 - vis_x0;
        let vis_h = vis_y1 - vis_y0;
        if vis_w <= 0.0 || vis_h <= 0.0 {
            return; // Nothing visible, skip rendering
        }

        // For 3D perspective, the shader handles UV mapping via the full dest_rect/source_rect;
        // we just need the scissor to be large enough. Use the full source rect.
        let (source_rect, dest_rect) = if transform_3d.is_some() {
            // Full source rect, original dest rect (shader applies perspective)
            let src_total_w = dest_size.0 / source_size.0 as f32;
            let src_total_h = dest_size.1 / source_size.1 as f32;
            (
                [0.0, 0.0, src_total_w.min(1.0), src_total_h.min(1.0)],
                [dest_pos.0, dest_pos.1, dest_size.0, dest_size.1],
            )
        } else {
            // Calculate source rect based on what portion is visible
            // Map visible region back to source texture coordinates
            let src_total_w = dest_size.0 / source_size.0 as f32;
            let src_total_h = dest_size.1 / source_size.1 as f32;

            // Calculate what portion of the dest rect is visible
            let vis_offset_x = vis_x0 - dest_pos.0;
            let vis_offset_y = vis_y0 - dest_pos.1;

            // Map to source texture coordinates
            let src_x0 = (vis_offset_x / dest_size.0) * src_total_w;
            let src_y0 = (vis_offset_y / dest_size.1) * src_total_h;
            let src_w = (vis_w / dest_size.0) * src_total_w;
            let src_h = (vis_h / dest_size.1) * src_total_h;

            (
                [
                    src_x0.min(1.0),
                    src_y0.min(1.0),
                    src_w.min(1.0),
                    src_h.min(1.0),
                ],
                [vis_x0, vis_y0, vis_w, vis_h],
            )
        };

        let (perspective_d, sin_rx, cos_rx, sin_ry, cos_ry) = if let Some(ref t3d) = transform_3d {
            (
                t3d.perspective_d,
                t3d.sin_rx,
                t3d.cos_rx,
                t3d.sin_ry,
                t3d.cos_ry,
            )
        } else {
            (0.0, 0.0, 1.0, 0.0, 1.0)
        };
        // 2D in-plane rotation. The flat (non-perspective) path in
        // LAYER_COMPOSITE_SHADER rotates `local_pos - (0.5, 0.5)`
        // around the center, then maps into dest_rect — so motion-
        // bound subtrees that need to spin (cn::spinner's
        // rotate_timeline) can rotate their cached texture per-frame
        // without re-baking.
        let sin_rz = rotate_z_rad.sin();
        let cos_rz = rotate_z_rad.cos();

        let uniforms = LayerCompositeUniforms {
            source_rect,
            dest_rect,
            viewport_size: [vp_w, vp_h],
            opacity,
            blend_mode: blend_mode as u32,
            clip_bounds,
            clip_radius,
            clip_type,
            perspective_d,
            sin_rx,
            cos_rx,
            sin_ry,
            cos_ry,
            sin_rz,
            cos_rz,
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Tight Blit Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let is_blend = blend_mode != blinc_core::BlendMode::Normal;

        // For non-Normal blend modes, snapshot the target so the shader can sample dest
        let dest_snapshot = if is_blend {
            let target_texture = target.texture();
            if target_texture
                .usage()
                .contains(wgpu::TextureUsages::COPY_SRC)
            {
                let temp =
                    self.layer_texture_cache
                        .acquire_exact(&self.device, self.viewport_size, false);

                let mut copy_encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Tight Blit Blend Dest Copy"),
                        });
                copy_encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: target_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &temp.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: self.viewport_size.0,
                        height: self.viewport_size.1,
                        depth_or_array_layers: 1,
                    },
                );
                self.queue.submit(std::iter::once(copy_encoder.finish()));
                Some(temp)
            } else {
                None
            }
        } else {
            None
        };

        let bind_group = if let Some(ref snapshot) = dest_snapshot {
            self.create_layer_composite_bind_group_with_dest(
                &uniform_buffer,
                source,
                &self.path_image_sampler,
                &snapshot.view,
                &self.path_image_sampler,
            )
        } else {
            self.create_layer_composite_bind_group(
                &uniform_buffer,
                source,
                &self.path_image_sampler,
            )
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tight Blit Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tight Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Set scissor rect to the visible region (already intersected with clip bounds)
            let scissor_x = vis_x0.max(0.0) as u32;
            let scissor_y = vis_y0.max(0.0) as u32;
            let scissor_w = vis_w.max(1.0) as u32;
            let scissor_h = vis_h.max(1.0) as u32;

            // Phase 4d Opt 2: when a damage scissor is set,
            // intersect it with the layer's visible bounds. The
            // composite paints only in the overlap of (layer
            // content area) ∩ (damage rect). Pixels outside the
            // damage rect stay from the previous frame's render —
            // which is what makes a scissored cache repaint
            // correct for CSS-animated layer effects.
            //
            // Empty intersection ⇒ skip the draw entirely (layer
            // doesn't touch the damage region this frame). Still
            // submit the otherwise-empty render pass so the encoder
            // doesn't leak — `LoadOp::Load` + no draw is a no-op
            // on the GPU.
            let scissor_and_draw = if let Some((dx, dy, dw, dh)) = self.pending_damage_scissor {
                let lx0 = scissor_x;
                let ly0 = scissor_y;
                let lx1 = lx0 + scissor_w;
                let ly1 = ly0 + scissor_h;
                let dx1 = dx + dw;
                let dy1 = dy + dh;
                let ix0 = lx0.max(dx);
                let iy0 = ly0.max(dy);
                let ix1 = lx1.min(dx1);
                let iy1 = ly1.min(dy1);
                if ix1 <= ix0 || iy1 <= iy0 {
                    None
                } else {
                    Some((ix0, iy0, ix1 - ix0, iy1 - iy0))
                }
            } else {
                Some((scissor_x, scissor_y, scissor_w, scissor_h))
            };

            if let Some((sx, sy, sw, sh)) = scissor_and_draw {
                render_pass.set_scissor_rect(sx, sy, sw, sh);
                render_pass.set_pipeline(&self.pipelines.layer_composite);
                render_pass.set_bind_group(0, &bind_group, &[]);
                render_pass.draw(0..6, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        if let Some(snapshot) = dest_snapshot {
            self.layer_texture_cache.release(snapshot);
        }
    }

    /// Blit a texture to the target with blending
    ///
    /// For non-Normal blend modes, copies the target to a temp texture first
    /// so the shader can read the destination for blend computation.
    fn blit_texture_to_target(
        &mut self,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) {
        use crate::primitives::LayerCompositeUniforms;

        let is_blend = blend_mode != blinc_core::BlendMode::Normal;

        // For non-Normal blend modes, copy the target to a temp texture
        // so the shader can sample the destination
        let dest_snapshot = if is_blend {
            if let Some(target_ptr) = self.blend_target_ptr {
                // Safety: pointer is valid for the duration of the render frame
                let target_texture = unsafe { &*target_ptr };
                let temp =
                    self.layer_texture_cache
                        .acquire(&self.device, self.viewport_size, false);

                let mut copy_encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Blend Dest Copy Encoder"),
                        });
                copy_encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: target_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &temp.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: self.viewport_size.0,
                        height: self.viewport_size.1,
                        depth_or_array_layers: 1,
                    },
                );
                self.queue.submit(std::iter::once(copy_encoder.finish()));
                Some(temp)
            } else {
                // No target texture available — fall back to Normal blend
                None
            }
        } else {
            None
        };

        // Full viewport blit - source covers entire texture, dest covers entire viewport
        let vp_w = self.viewport_size.0 as f32;
        let vp_h = self.viewport_size.1 as f32;
        let effective_blend = if dest_snapshot.is_some() {
            blend_mode
        } else {
            blinc_core::BlendMode::Normal
        };
        let uniforms = LayerCompositeUniforms {
            source_rect: [0.0, 0.0, 1.0, 1.0], // Full texture (normalized)
            dest_rect: [0.0, 0.0, vp_w, vp_h],
            viewport_size: [vp_w, vp_h],
            opacity,
            blend_mode: effective_blend as u32,
            clip_bounds: [0.0, 0.0, vp_w, vp_h], // No clipping
            clip_radius: [0.0, 0.0, 0.0, 0.0],
            clip_type: 0,
            perspective_d: 0.0,
            sin_rx: 0.0,
            cos_rx: 1.0,
            sin_ry: 0.0,
            cos_ry: 1.0,
            sin_rz: 0.0,
            cos_rz: 1.0,
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Blit Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = if let Some(ref snapshot) = dest_snapshot {
            self.create_layer_composite_bind_group_with_dest(
                &uniform_buffer,
                source,
                &self.path_image_sampler,
                &snapshot.view,
                &self.path_image_sampler,
            )
        } else {
            self.create_layer_composite_bind_group(
                &uniform_buffer,
                source,
                &self.path_image_sampler,
            )
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blit Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        // Load existing content - we're blending on top
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.layer_composite);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Release the dest snapshot texture
        if let Some(snapshot) = dest_snapshot {
            self.layer_texture_cache.release(snapshot);
        }
    }

    /// Blit a specific region from source texture to target at given position
    ///
    /// This is used for layer effects where we need to composite only the
    /// element's region back to the target at the correct position.
    fn blit_region_to_target(
        &mut self,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        position: (f32, f32),
        size: (f32, f32),
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) {
        self.blit_region_to_target_with_clip(
            source, target, position, size, opacity, blend_mode, None,
        )
    }

    /// Blit a specific region with optional clip
    #[allow(clippy::too_many_arguments)]
    fn blit_region_to_target_with_clip(
        &mut self,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        position: (f32, f32),
        size: (f32, f32),
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
        clip: Option<([f32; 4], [f32; 4])>, // (bounds, radii)
    ) {
        use crate::primitives::LayerCompositeUniforms;

        let vp_w = self.viewport_size.0 as f32;
        let vp_h = self.viewport_size.1 as f32;

        // Source rect in normalized coordinates (0-1)
        // The source texture is viewport-sized, so we extract the element's region
        let source_rect = [
            position.0 / vp_w,
            position.1 / vp_h,
            size.0 / vp_w,
            size.1 / vp_h,
        ];

        // Dest rect in viewport pixel coordinates
        let dest_rect = [position.0, position.1, size.0, size.1];

        let mut uniforms = LayerCompositeUniforms {
            source_rect,
            dest_rect,
            viewport_size: [vp_w, vp_h],
            opacity,
            blend_mode: blend_mode as u32,
            clip_bounds: [0.0, 0.0, vp_w, vp_h],
            clip_radius: [0.0, 0.0, 0.0, 0.0],
            clip_type: 0,
            perspective_d: 0.0,
            sin_rx: 0.0,
            cos_rx: 1.0,
            sin_ry: 0.0,
            cos_ry: 1.0,
            sin_rz: 0.0,
            cos_rz: 1.0,
        };

        if let Some((bounds, radii)) = clip {
            uniforms.clip_bounds = bounds;
            uniforms.clip_radius = radii;
            uniforms.clip_type = 1;
        }

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Region Blit Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = self.create_layer_composite_bind_group(
            &uniform_buffer,
            source,
            &self.path_image_sampler,
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Region Blit Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Region Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Set scissor rect to only affect the element's region
            render_pass.set_scissor_rect(
                position.0.max(0.0) as u32,
                position.1.max(0.0) as u32,
                size.0.min(vp_w - position.0).max(1.0) as u32,
                size.1.min(vp_h - position.1).max(1.0) as u32,
            );

            render_pass.set_pipeline(&self.pipelines.layer_composite);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // SDF 3D Viewport Rendering
    // ─────────────────────────────────────────────────────────────────────────────

    /// Initialize SDF 3D resources lazily
    fn ensure_sdf_3d_resources(&mut self) {
        if self.sdf_3d_resources.is_some() {
            return;
        }

        // Create bind group layout for SDF 3D uniforms
        let bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("SDF 3D Bind Group Layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        // Create uniform buffer
        let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SDF 3D Uniform Buffer"),
            size: std::mem::size_of::<Sdf3DUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF 3D Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        self.sdf_3d_resources = Some(Sdf3DResources {
            bind_group_layout,
            uniform_buffer,
            bind_group,
            pipeline_cache: HashMap::new(),
        });
    }

    /// Get or create a render pipeline for an SDF 3D viewport
    fn get_or_create_sdf_3d_pipeline(&mut self, shader_wgsl: &str) -> u64 {
        self.ensure_sdf_3d_resources();

        // Hash the shader for caching
        let shader_hash = Self::hash_string(shader_wgsl);

        let resources = self.sdf_3d_resources.as_mut().unwrap();

        if !resources.pipeline_cache.contains_key(&shader_hash) {
            // Create shader module
            let shader_module = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("SDF 3D Raymarch Shader"),
                    source: wgpu::ShaderSource::Wgsl(shader_wgsl.into()),
                });

            // Create pipeline layout
            let pipeline_layout =
                self.device
                    .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("SDF 3D Pipeline Layout"),
                        bind_group_layouts: &[&resources.bind_group_layout],
                        push_constant_ranges: &[],
                    });

            // Create render pipeline
            let pipeline = self
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("SDF 3D Raymarch Pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader_module,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader_module,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: self.texture_format,
                            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });

            resources.pipeline_cache.insert(shader_hash, pipeline);
        }

        shader_hash
    }

    /// Simple string hash for shader caching
    fn hash_string(s: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }

    /// Render SDF 3D viewports to the target
    pub fn render_sdf_3d_viewports(
        &mut self,
        target: &wgpu::TextureView,
        viewports: &[Viewport3D],
    ) {
        if viewports.is_empty() {
            return;
        }

        self.ensure_sdf_3d_resources();

        let (surface_width, surface_height) = self.viewport_size;

        for viewport in viewports {
            // The paint context already clipped to its clip stack, but we need to
            // further clamp to the render target bounds for wgpu validity.
            // If we need to clamp further, we must also adjust the UV offset/scale.
            let orig_x = viewport.bounds[0];
            let orig_y = viewport.bounds[1];
            let orig_w = viewport.bounds[2];
            let orig_h = viewport.bounds[3];

            // Clamp to render target bounds
            let x = orig_x.max(0.0);
            let y = orig_y.max(0.0);
            let right = (orig_x + orig_w).min(surface_width as f32);
            let bottom = (orig_y + orig_h).min(surface_height as f32);
            let w = (right - x).max(0.0);
            let h = (bottom - y).max(0.0);

            // Skip if viewport is fully outside the render target or has zero size
            if w <= 0.0 || h <= 0.0 {
                continue;
            }

            // Check if we needed to clamp further and adjust UV accordingly
            let mut uniforms = viewport.uniforms;
            if orig_w > 0.0 && orig_h > 0.0 {
                // Calculate additional UV adjustment for surface clamping
                // The paint context's UV maps the paint-clipped region to the original viewport.
                // If we clamp further here, we need to adjust those UVs.
                let extra_offset_x = (x - orig_x) / orig_w;
                let extra_offset_y = (y - orig_y) / orig_h;
                let extra_scale_x = w / orig_w;
                let extra_scale_y = h / orig_h;

                // Compose with existing UV transform: new_uv = old_offset + (extra_offset + uv * extra_scale) * old_scale
                // Which simplifies to: new_offset = old_offset + extra_offset * old_scale, new_scale = old_scale * extra_scale
                uniforms.uv_offset[0] += extra_offset_x * uniforms.uv_scale[0];
                uniforms.uv_offset[1] += extra_offset_y * uniforms.uv_scale[1];
                uniforms.uv_scale[0] *= extra_scale_x;
                uniforms.uv_scale[1] *= extra_scale_y;
            }

            // Get or create pipeline for this viewport's shader
            let shader_hash = self.get_or_create_sdf_3d_pipeline(&viewport.shader_wgsl);

            // Update uniforms with adjusted UV
            let resources = self.sdf_3d_resources.as_ref().unwrap();
            self.queue
                .write_buffer(&resources.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

            // Create command encoder
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("SDF 3D Render Encoder"),
                });

            // Render pass
            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("SDF 3D Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            // Don't clear - we're rendering on top of existing content
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                // Set viewport and scissor to the clamped bounds
                render_pass.set_viewport(x, y, w, h, 0.0, 1.0);
                render_pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);

                let resources = self.sdf_3d_resources.as_ref().unwrap();
                let pipeline = resources.pipeline_cache.get(&shader_hash).unwrap();
                render_pass.set_pipeline(pipeline);
                render_pass.set_bind_group(0, &resources.bind_group, &[]);
                render_pass.draw(0..3, 0..1); // Fullscreen triangle
            }

            // Submit
            self.queue.submit(std::iter::once(encoder.finish()));
        }
    }

    /// Render GPU particle viewports
    pub fn render_particle_viewports(
        &mut self,
        target: &wgpu::TextureView,
        viewports: &[crate::primitives::ParticleViewport3D],
    ) {
        use crate::particles::{ParticleSystemGpu, ParticleViewport};
        use std::hash::{Hash, Hasher};

        if viewports.is_empty() {
            return;
        }

        // Particles require compute shaders (for the simulation pass) and
        // storage buffers (for the particle buffer). WebGL2 has neither,
        // so skip particle rendering entirely in DT/Tier-3 mode.
        if !self.has_storage_buffers {
            return;
        }

        // Use the actual texture format that was selected during renderer initialization
        let surface_format = self.texture_format;

        for (vp_index, vp) in viewports.iter().enumerate() {
            if !vp.playing {
                continue;
            }

            // Generate a stable hash key for this particle system based on emitter config
            // This allows us to reuse the same GPU buffers across frames
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            vp_index.hash(&mut hasher);
            vp.max_particles.hash(&mut hasher);
            // Hash emitter position components to differentiate systems at different positions
            (vp.emitter.position_shape[0].to_bits()).hash(&mut hasher);
            (vp.emitter.position_shape[1].to_bits()).hash(&mut hasher);
            (vp.emitter.position_shape[2].to_bits()).hash(&mut hasher);
            let system_key = hasher.finish();

            // Get or create the particle system
            let system = self.particle_systems.entry(system_key).or_insert_with(|| {
                ParticleSystemGpu::new(&self.device, surface_format, vp.max_particles)
            });

            // Convert ParticleViewport3D to ParticleViewport for the GPU system
            let particle_viewport = ParticleViewport {
                emitter: vp.emitter,
                forces: vp.forces.clone(),
                max_particles: vp.max_particles,
                camera_pos: vp.camera_pos,
                camera_target: vp.camera_target,
                camera_up: vp.camera_up,
                fov: vp.fov,
                time: vp.time,
                delta_time: vp.delta_time,
                bounds: vp.bounds,
                blend_mode: vp.blend_mode,
                playing: vp.playing,
            };

            // Create command encoder
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Particle Encoder"),
                });

            // Run compute pass to update particles
            system.update(&self.queue, &mut encoder, &particle_viewport);

            // Submit compute work first
            self.queue.submit(std::iter::once(encoder.finish()));

            // Create render encoder
            let mut render_encoder =
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Particle Render Encoder"),
                    });

            // Render pass
            {
                let mut render_pass =
                    render_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Particle Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: target,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load, // Don't clear, draw on top
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                // Set viewport to the particle bounds
                render_pass.set_viewport(
                    vp.bounds[0],
                    vp.bounds[1],
                    vp.bounds[2],
                    vp.bounds[3],
                    0.0,
                    1.0,
                );

                // Render the particles
                system.render(&self.queue, &mut render_pass, &particle_viewport);
            }

            // Submit render work
            self.queue.submit(std::iter::once(render_encoder.finish()));
        }
    }
}

impl Default for GpuRenderer {
    fn default() -> Self {
        // Create a basic renderer synchronously using pollster
        pollster::block_on(Self::new(RendererConfig::default()))
            .expect("Failed to create default renderer")
    }
}

/// Inverse of a column-major 4×4 matrix (GLU-style cofactor expansion).
pub(crate) fn mat4_inverse_flat(m: &[f32; 16]) -> [f32; 16] {
    let mut inv = [0.0f32; 16];
    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
        + m[9] * m[7] * m[14]
        + m[13] * m[6] * m[11]
        - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
        - m[8] * m[7] * m[14]
        - m[12] * m[6] * m[11]
        + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
        + m[8] * m[7] * m[13]
        + m[12] * m[5] * m[11]
        - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
        - m[8] * m[6] * m[13]
        - m[12] * m[5] * m[10]
        + m[12] * m[6] * m[9];
    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
        - m[9] * m[3] * m[14]
        - m[13] * m[2] * m[11]
        + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
        + m[8] * m[3] * m[14]
        + m[12] * m[2] * m[11]
        - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
        - m[8] * m[3] * m[13]
        - m[12] * m[1] * m[11]
        + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
        + m[8] * m[2] * m[13]
        + m[12] * m[1] * m[10]
        - m[12] * m[2] * m[9];
    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
        + m[5] * m[3] * m[14]
        + m[13] * m[2] * m[7]
        - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
        - m[4] * m[3] * m[14]
        - m[12] * m[2] * m[7]
        + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
        + m[4] * m[3] * m[13]
        + m[12] * m[1] * m[7]
        - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
        - m[4] * m[2] * m[13]
        - m[12] * m[1] * m[6]
        + m[12] * m[2] * m[5];
    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
        - m[5] * m[3] * m[10]
        - m[9] * m[2] * m[7]
        + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
        + m[4] * m[3] * m[10]
        + m[8] * m[2] * m[7]
        - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
        - m[4] * m[3] * m[9]
        - m[8] * m[1] * m[7]
        + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
        + m[4] * m[2] * m[9]
        + m[8] * m[1] * m[6]
        - m[8] * m[2] * m[5];
    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-12 {
        return [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
    }
    let id = 1.0 / det;
    for v in &mut inv {
        *v *= id;
    }
    inv
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────────────────────────────────────────────────────────────────
    // LayerTextureCache Tests
    // ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn layer_texture_cache_initial_state() {
        let cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);
        assert_eq!(cache.pool_size(), 0);
        assert_eq!(cache.named_count(), 0);
    }

    #[test]
    fn layer_texture_cache_clear_all() {
        let cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);
        // Pool is empty, but clear_all should work without panic
        let mut cache = cache;
        cache.clear_all();
        assert_eq!(cache.pool_size(), 0);
        assert_eq!(cache.named_count(), 0);
    }

    #[test]
    fn layer_texture_cache_format_preserved() {
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let cache = LayerTextureCache::new(format);
        assert_eq!(cache.format, format);
    }

    #[test]
    fn layer_texture_matches_size() {
        // Test requires GPU, but we can test the matches_size logic
        // by creating a helper struct with known sizes
        struct FakeTexture {
            size: (u32, u32),
        }
        impl FakeTexture {
            fn matches_size(&self, size: (u32, u32)) -> bool {
                self.size == size
            }
        }

        let tex = FakeTexture { size: (800, 600) };
        assert!(tex.matches_size((800, 600)));
        assert!(!tex.matches_size((800, 601)));
        assert!(!tex.matches_size((801, 600)));
        assert!(!tex.matches_size((400, 300)));
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // GPU Integration Tests (require actual wgpu device)
    // ─────────────────────────────────────────────────────────────────────────────

    /// Helper to create a test wgpu device
    async fn create_test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .ok()?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .ok()?;

        Some((device, queue))
    }

    /// Helper to create unique layer IDs for testing
    fn test_layer_id(id: u64) -> blinc_core::LayerId {
        blinc_core::LayerId::new(id)
    }

    #[test]
    fn layer_texture_cache_acquire_and_release() {
        pollster::block_on(async {
            let Some((device, _queue)) = create_test_device().await else {
                // Skip test if no GPU available
                return;
            };

            let mut cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);

            // Acquire a texture
            let tex1 = cache.acquire(&device, (512, 512), false);
            assert_eq!(tex1.size, (512, 512));
            assert!(!tex1.has_depth);

            // Release it back to pool
            cache.release(tex1);
            assert_eq!(cache.pool_size(), 1);

            // Acquire again - should reuse from pool
            let tex2 = cache.acquire(&device, (512, 512), false);
            assert_eq!(tex2.size, (512, 512));
            assert_eq!(cache.pool_size(), 0); // Removed from pool

            // Acquire different size in different bucket - should create new
            // Note: Using 256x256 (Medium bucket) since XLarge (>512) is not pooled
            let tex3 = cache.acquire(&device, (256, 256), false);
            assert_eq!(tex3.size, (256, 256));
            assert_eq!(cache.pool_size(), 0);

            // Release both - tex2 goes to Large bucket, tex3 goes to Medium bucket
            cache.release(tex2);
            cache.release(tex3);
            assert_eq!(cache.pool_size(), 2);
        });
    }

    #[test]
    fn layer_texture_cache_named_textures() {
        pollster::block_on(async {
            let Some((device, _queue)) = create_test_device().await else {
                return;
            };

            let mut cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);
            let layer_id = test_layer_id(1);

            // Store a named texture
            let tex = cache.acquire(&device, (256, 256), false);
            cache.store(layer_id, tex);
            assert_eq!(cache.named_count(), 1);

            // Get reference to it
            let retrieved = cache.get(&layer_id);
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().size, (256, 256));

            // Remove it
            let removed = cache.remove(&layer_id);
            assert!(removed.is_some());
            assert_eq!(cache.named_count(), 0);

            // Release back to pool
            cache.release(removed.unwrap());
            assert_eq!(cache.pool_size(), 1);
        });
    }

    #[test]
    fn layer_texture_cache_clear_named_releases_to_pool() {
        pollster::block_on(async {
            let Some((device, _queue)) = create_test_device().await else {
                return;
            };

            let mut cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);

            // Store several named textures
            for i in 0..3 {
                let tex = cache.acquire(&device, (128, 128), false);
                cache.store(test_layer_id(i + 100), tex);
            }
            assert_eq!(cache.named_count(), 3);
            assert_eq!(cache.pool_size(), 0);

            // Clear named - should release to pool (capped at max_per_bucket=2)
            cache.clear_named();
            assert_eq!(cache.named_count(), 0);
            assert_eq!(cache.pool_size(), 2);
        });
    }

    #[test]
    fn layer_texture_cache_pool_size_limit() {
        pollster::block_on(async {
            let Some((device, _queue)) = create_test_device().await else {
                return;
            };

            let mut cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);
            // Default max_per_bucket is 4 (bucketed by size: Small/Medium/Large)

            // Acquire and release more than max_per_bucket textures in Small bucket (64x64)
            let mut textures = Vec::new();
            for _ in 0..8 {
                textures.push(cache.acquire(&device, (64, 64), false));
            }

            // Release all
            for tex in textures {
                cache.release(tex);
            }

            // Pool should be capped at max_per_bucket (2) for the Small bucket
            assert_eq!(cache.pool_size(), 2);
        });
    }

    #[test]
    fn layer_texture_with_depth() {
        pollster::block_on(async {
            let Some((device, _queue)) = create_test_device().await else {
                return;
            };

            let mut cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);

            // Acquire texture with depth
            let tex_with_depth = cache.acquire(&device, (512, 512), true);
            assert!(tex_with_depth.has_depth);
            assert!(tex_with_depth.depth_view.is_some());

            // Acquire texture without depth
            let tex_no_depth = cache.acquire(&device, (512, 512), false);
            assert!(!tex_no_depth.has_depth);
            assert!(tex_no_depth.depth_view.is_none());

            // Release both
            cache.release(tex_with_depth);
            cache.release(tex_no_depth);
            assert_eq!(cache.pool_size(), 2);

            // Acquire with depth - should NOT get the one without depth
            let tex_reacquire = cache.acquire(&device, (512, 512), true);
            assert!(tex_reacquire.has_depth);
            assert_eq!(cache.pool_size(), 1); // The no-depth one remains
        });
    }

    #[test]
    fn layer_texture_reuse_larger() {
        pollster::block_on(async {
            let Some((device, _queue)) = create_test_device().await else {
                return;
            };

            let mut cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);

            // Acquire and release a Large bucket texture (512x512)
            // Note: XLarge (>512) is not pooled, so we use 512x512
            let large_tex = cache.acquire(&device, (512, 512), false);
            cache.release(large_tex);
            assert_eq!(cache.pool_size(), 1);

            // Acquire smaller from Medium bucket - should still reuse from Large bucket
            let small_tex = cache.acquire(&device, (256, 256), false);
            // The actual size will be 512x512 (reused from Large pool)
            assert!(small_tex.size.0 >= 256 && small_tex.size.1 >= 256);
            assert_eq!(cache.pool_size(), 0);
        });
    }
}
