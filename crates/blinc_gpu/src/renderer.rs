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
    GLASS_DT_SHADER, GLASS_SHADER, GLOW_SHADER, IMAGE_SHADER, LAYER_COMPOSITE_SHADER,
    MASK_IMAGE_SHADER, MESH_DT_SHADER, PATH_SHADER, SDF_3D_DT_SHADER, SDF_3D_SHADER,
    SDF_3D_VB_SHADER, SDF_CORE_DT_SHADER, SDF_CORE_SHADER, SDF_CORE_VB_SHADER, SDF_NOTCH_DT_SHADER,
    SDF_NOTCH_SHADER, SDF_NOTCH_VB_SHADER, SDF_SHADER, SDF_SHADOW_DT_SHADER, SDF_SHADOW_SHADER,
    SDF_SHADOW_VB_SHADER, SIMPLE_GLASS_DT_SHADER, SIMPLE_GLASS_SHADER, TEXT_DT_SHADER, TEXT_SHADER,
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
    features
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
            max_primitives: 1_000,    // ~192 KB — handles complex UI screens
            max_glass_primitives: 32, // ~8 KB
            max_glyphs: 4_000,        // ~256 KB — handles full-screen text content
            sample_count: 1,
            texture_format: None,
            unified_text_rendering: true, // Enabled for consistent transforms during animations
            gpu_memory_budget: budget_mb * 1024 * 1024,
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
    /// Pipeline for SDF primitives (rects, circles, etc.) — monolithic fallback (deprecated)
    #[allow(dead_code)]
    sdf: wgpu::RenderPipeline,
    /// Pipeline for SDF primitives rendering on top of existing content (1x sampled) — monolithic fallback (deprecated)
    #[allow(dead_code)]
    sdf_overlay: wgpu::RenderPipeline,
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
    /// Split SDF overlay pipeline: shadow shapes (1x sampled)
    sdf_shadow_overlay: wgpu::RenderPipeline,
    /// Split SDF overlay pipeline: 3D raymarched shapes (1x sampled)
    sdf_3d_overlay: wgpu::RenderPipeline,
    /// Split SDF overlay pipeline: notch shapes (1x sampled)
    sdf_notch_overlay: wgpu::RenderPipeline,
    /// Pipeline for text rendering (MSAA)
    #[allow(dead_code)]
    text: wgpu::RenderPipeline,
    /// Pipeline for text rendering on top of existing content (1x sampled)
    text_overlay: wgpu::RenderPipeline,
    /// Pipeline for final compositing (MSAA)
    #[allow(dead_code)]
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
    /// SDF pipeline for this sample count (monolithic fallback, deprecated)
    #[allow(dead_code)]
    sdf: wgpu::RenderPipeline,
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
    /// Width = 23 texels (one per vec4 field of GpuPrimitive), height = max_primitives.
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

/// Cached MSAA textures and resources for overlay rendering
struct CachedMsaaTextures {
    msaa_texture: wgpu::Texture,
    msaa_view: wgpu::TextureView,
    resolve_texture: wgpu::Texture,
    resolve_view: wgpu::TextureView,
    width: u32,
    height: u32,
    sample_count: u32,
    /// Sampler for compositing (reused across frames)
    sampler: wgpu::Sampler,
    /// Uniform buffer for compositing (reused across frames)
    composite_uniform_buffer: wgpu::Buffer,
    /// Bind group for compositing (recreated when textures change)
    composite_bind_group: wgpu::BindGroup,
}

/// Cached glass resources to avoid per-frame allocations
struct CachedGlassResources {
    /// Sampler for backdrop texture (reused across frames)
    sampler: wgpu::Sampler,
    /// Cached bind group (valid when backdrop texture hasn't changed)
    bind_group: Option<wgpu::BindGroup>,
    /// Width/height when bind group was created (for invalidation)
    bind_group_size: (u32, u32),
}

/// Cached text resources to avoid per-frame allocations
struct CachedTextResources {
    /// Cached bind group (valid when atlas texture view hasn't changed)
    bind_group: wgpu::BindGroup,
    /// Pointer to grayscale atlas view when bind group was created (for invalidation)
    atlas_view_ptr: *const wgpu::TextureView,
    /// Pointer to color atlas view when bind group was created (for invalidation)
    color_atlas_view_ptr: *const wgpu::TextureView,
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
    depth_texture: Option<wgpu::Texture>,
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
    fn from_size(size: (u32, u32)) -> Self {
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
    fn max_size(&self) -> u32 {
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
    named_textures: std::collections::HashMap<blinc_core::LayerId, LayerTexture>,
    /// Size-bucketed pools for efficient texture reuse
    pool_small: Vec<LayerTexture>, // <= 128
    pool_medium: Vec<LayerTexture>, // <= 256
    pool_large: Vec<LayerTexture>,  // <= 512
    pool_xlarge: Vec<LayerTexture>, // > 512
    /// Texture format used for all layer textures
    format: wgpu::TextureFormat,
    /// Maximum textures per bucket
    max_per_bucket: usize,
    /// Cache statistics
    stats: TextureCacheStats,
    /// Number of consecutive frames without a successful `acquire()`.
    /// `evict_oversized()` (called once per frame from the renderer)
    /// increments this; `acquire()` resets it. After
    /// `IDLE_DROP_THRESHOLD` frames, drop one texture from each pool.
    /// After `IDLE_FULL_FLUSH` frames, drop everything. This reclaims
    /// the GPU memory held by layer/glass/blur intermediates when the
    /// UI hasn't needed them for a while.
    idle_frames: u32,
}

/// Drop one texture per pool after this many idle frames (~1 s at 60fps).
const IDLE_DROP_THRESHOLD: u32 = 60;
/// Drop the entire pool after this many idle frames (~2 s at 60fps).
const IDLE_FULL_FLUSH: u32 = 120;

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
    fn estimate_texture_bytes(size: (u32, u32), has_depth: bool) -> u64 {
        let color_bytes = (size.0 as u64) * (size.1 as u64) * 4;
        let depth_bytes = if has_depth {
            (size.0 as u64) * (size.1 as u64) * 4 // Depth32Float = 4 bytes
        } else {
            0
        };
        color_bytes + depth_bytes
    }

    /// Get the appropriate pool for a bucket
    fn get_pool(&self, bucket: TextureSizeBucket) -> &Vec<LayerTexture> {
        match bucket {
            TextureSizeBucket::Small => &self.pool_small,
            TextureSizeBucket::Medium => &self.pool_medium,
            TextureSizeBucket::Large => &self.pool_large,
            TextureSizeBucket::XLarge => &self.pool_xlarge,
        }
    }

    /// Get mutable pool for a bucket
    fn get_pool_mut(&mut self, bucket: TextureSizeBucket) -> &mut Vec<LayerTexture> {
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
    fn update_pool_stats(&mut self) {
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
    fn update_named_stats(&mut self) {
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
    cached_msaa: Option<CachedMsaaTextures>,
    /// Cached glass resources (avoids per-frame allocation)
    cached_glass: Option<CachedGlassResources>,
    /// Cached text resources (avoids per-frame allocation)
    cached_text: Option<CachedTextResources>,
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
    /// Cached @flow GPU pipelines (compiled lazily from FlowGraph → WGSL)
    flow_pipeline_cache: crate::flow_pipeline::FlowPipelineCache,
    /// Staging texture for scene capture (used by flow shaders with sample_scene()).
    /// Lazily created/resized to match the render target.
    scene_copy_texture: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
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
    bind_group_layout: wgpu::BindGroupLayout,
    instance_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
}

/// Shadow map resolution (square)
pub(crate) const SHADOW_MAP_SIZE: u32 = 2048;

use crate::mesh_pipeline::{
    MeshBufferCacheEntry, MeshPipeline, MAX_MORPH_TARGETS, MESH_CACHE_CAPACITY,
    MORPH_CACHE_CAPACITY,
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

    /// Get the preferred backend for the current platform
    ///
    /// Using the primary backend instead of all backends reduces memory usage
    /// by avoiding initialization of multiple GPU driver stacks.
    fn preferred_backends() -> wgpu::Backends {
        #[cfg(target_os = "macos")]
        {
            wgpu::Backends::METAL
        }
        #[cfg(target_os = "windows")]
        {
            wgpu::Backends::DX12
        }
        #[cfg(target_os = "linux")]
        {
            wgpu::Backends::VULKAN
        }
        #[cfg(target_arch = "wasm32")]
        {
            wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_arch = "wasm32"
        )))]
        {
            wgpu::Backends::PRIMARY
        }
    }

    /// Write only the given subranges of `primitives` to the GPU
    /// storage buffer, leaving the rest as it was after the previous
    /// upload. Used by the compositor fast path: when an animation
    /// frame patched only the primitives covered by a handful of
    /// motion-bound subtrees, we re-upload just those byte ranges
    /// (each one `range.len() × sizeof::<GpuPrimitive>()` bytes) via
    /// `queue.write_buffer(offset, slice)` instead of pushing the
    /// whole batch.
    ///
    /// Caller guarantees:
    ///  - `primitives` is the SAME-length view as what was uploaded
    ///    on the previous full paint (no insertions / deletions).
    ///  - Each range is in-bounds (`end <= primitives.len()`).
    ///  - DT-mode (no storage buffers) is **not** in use — partial
    ///    upload via `write_texture` isn't worth the complexity for
    ///    a fallback path; fall back to the full
    ///    `write_primitives_safe` if `has_storage_buffers` is false.
    ///
    /// Cost (cn_demo benchmark): full upload was ~100 KB / frame
    /// (~150 µs of `queue.write_buffer` overhead). A typical fast-path
    /// frame patches a single binding's range — for the
    /// `cn::progress_animated` indicator that's <10 primitives or
    /// ~2.5 KB / frame, putting the write closer to ~5 µs.
    pub fn write_primitives_partial(
        &self,
        primitives: &[GpuPrimitive],
        ranges: &[std::ops::Range<usize>],
    ) {
        if primitives.is_empty() || ranges.is_empty() || !self.has_storage_buffers {
            return;
        }
        let stride = std::mem::size_of::<GpuPrimitive>() as u64;
        let max_primitives = self.config.max_primitives;
        for range in ranges {
            // Clip to the same capacity bounds `write_primitives_safe`
            // uses for the full upload — keeps both paths consistent
            // when an app accidentally over-emits.
            let end = range.end.min(primitives.len()).min(max_primitives);
            if range.start >= end {
                continue;
            }
            let slice = &primitives[range.start..end];
            let bytes = bytemuck::cast_slice::<GpuPrimitive, u8>(slice);
            let offset_bytes = (range.start as u64) * stride;
            self.queue
                .write_buffer(&self.buffers.primitives, offset_bytes, bytes);
        }
    }

    /// Safely write primitives to buffer, truncating if necessary to prevent overflow
    fn write_primitives_safe(&self, primitives: &[GpuPrimitive]) {
        if primitives.is_empty() {
            return;
        }
        let max_primitives = self.config.max_primitives;
        let primitives_to_write = if primitives.len() > max_primitives {
            tracing::warn!(
                "Primitive count {} exceeds buffer capacity {}, truncating",
                primitives.len(),
                max_primitives
            );
            &primitives[..max_primitives]
        } else {
            primitives
        };

        if !self.has_storage_buffers {
            // DT mode: upload to data texture instead of storage buffer.
            // Each GpuPrimitive is 23 × vec4<f32> = 23 RGBA32F texels in a row.
            if let Some(ref tex) = self.buffers.prim_data_texture {
                let bytes = bytemuck::cast_slice::<GpuPrimitive, u8>(primitives_to_write);
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    bytes,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        // 23 texels × 16 bytes per RGBA32F texel = 368 bytes per row
                        bytes_per_row: Some(23 * 16),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width: 23,
                        height: primitives_to_write.len() as u32,
                        depth_or_array_layers: 1,
                    },
                );
            }
        } else {
            // Tier 1/2: write to storage buffer as before
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(primitives_to_write),
            );
        }
    }

    /// Sort primitives by `SdfPipelineCategory` and compute contiguous ranges.
    ///
    /// Returns a new sorted `Vec` and the corresponding `SdfPrimitiveRanges`.
    /// Text primitives are included in the sorted output (and tracked in ranges)
    /// but should NOT be drawn by the split SDF pipelines — they use the separate
    /// text pipeline.
    fn sort_primitives_by_category(
        primitives: &[GpuPrimitive],
    ) -> (Vec<GpuPrimitive>, SdfPrimitiveRanges) {
        if primitives.is_empty() {
            return (Vec::new(), SdfPrimitiveRanges::default());
        }
        let mut sorted: Vec<GpuPrimitive> = primitives.to_vec();
        sorted.sort_by_key(|p| p.pipeline_category());

        let mut ranges = SdfPrimitiveRanges::default();
        let mut i = 0u32;
        let len = sorted.len() as u32;
        while i < len {
            let cat = sorted[i as usize].pipeline_category();
            let start = i;
            while i < len && sorted[i as usize].pipeline_category() == cat {
                i += 1;
            }
            let range = start..i;
            match cat {
                SdfPipelineCategory::Core => ranges.core = range,
                SdfPipelineCategory::Shadow => ranges.shadow = range,
                SdfPipelineCategory::Sdf3D => ranges.sdf_3d = range,
                SdfPipelineCategory::Notch => ranges.notch = range,
                SdfPipelineCategory::Text => ranges.text = range,
            }
        }
        (sorted, ranges)
    }

    /// Sort primitives, upload to the GPU buffer (with safety truncation), and return ranges.
    ///
    /// When `has_vertex_storage` is `false`, also builds and uploads the
    /// `SdfVertexInstance` buffer used by the VB fallback shaders.
    fn upload_sorted_primitives(&mut self, primitives: &[GpuPrimitive]) -> SdfPrimitiveRanges {
        if primitives.is_empty() {
            return SdfPrimitiveRanges::default();
        }
        // Preserve instance-index z-order. Scan once to (a) populate the
        // per-category ranges (used for has-text / has-shadow checks) and
        // (b) build the contiguous-run list that `draw_split_sdf` iterates
        // in order so shadow/3D/notch don't get overdrawn by later core
        // primitives (app backgrounds, section containers).
        let mut ranges = SdfPrimitiveRanges::default();
        let len = primitives.len() as u32;
        let full = 0..len;
        for p in primitives {
            match p.pipeline_category() {
                SdfPipelineCategory::Core => {
                    if ranges.core.is_empty() {
                        ranges.core = full.clone();
                    }
                }
                SdfPipelineCategory::Shadow => {
                    if ranges.shadow.is_empty() {
                        ranges.shadow = full.clone();
                    }
                }
                SdfPipelineCategory::Sdf3D => {
                    if ranges.sdf_3d.is_empty() {
                        ranges.sdf_3d = full.clone();
                    }
                }
                SdfPipelineCategory::Notch => {
                    if ranges.notch.is_empty() {
                        ranges.notch = full.clone();
                    }
                }
                SdfPipelineCategory::Text => {
                    if ranges.text.is_empty() {
                        ranges.text = full.clone();
                    }
                }
            }
        }
        ranges.runs = Self::compute_category_runs(primitives);
        self.write_primitives_safe(primitives);

        // VERTEX_STORAGE fallback: build instance data and upload to VB
        if !self.has_vertex_storage {
            let instances: Vec<SdfVertexInstance> = primitives
                .iter()
                .map(SdfVertexInstance::from_primitive)
                .collect();
            let bytes = bytemuck::cast_slice::<SdfVertexInstance, u8>(&instances);
            let needed = bytes.len() as u64;

            // Create or resize the vertex buffer if necessary
            let needs_new_buffer = match &self.buffers.sdf_vertex_instances {
                Some(buf) => buf.size() < needed,
                None => true,
            };
            if needs_new_buffer {
                self.buffers.sdf_vertex_instances =
                    Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("SDF Vertex Instances (VB Fallback)"),
                        size: needed,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }));
            }
            if let Some(buf) = &self.buffers.sdf_vertex_instances {
                self.queue.write_buffer(buf, 0, bytes);
            }
        }

        ranges
    }

    /// Returns the SDF vertex instance buffer for the VB fallback path,
    /// or `None` when VERTEX_STORAGE is supported.
    fn sdf_vb_buffer(&self) -> Option<&wgpu::Buffer> {
        if self.has_vertex_storage {
            None
        } else {
            self.buffers.sdf_vertex_instances.as_ref()
        }
    }

    /// Issue draw calls for split SDF pipelines using pre-computed ranges.
    ///
    /// The bind group must already be set on the render pass before calling this.
    /// If `overlay` is true, the overlay pipeline variants are used (1x sampled).
    /// When `vb_buffer` is `Some`, the instance vertex buffer is bound at slot 0
    /// before each pipeline draw (VERTEX_STORAGE fallback path).
    fn draw_split_sdf<'a>(
        render_pass: &mut wgpu::RenderPass<'a>,
        pipelines: &'a Pipelines,
        ranges: &SdfPrimitiveRanges,
        overlay: bool,
        vb_buffer: Option<&'a wgpu::Buffer>,
    ) {
        if let Some(buf) = vb_buffer {
            render_pass.set_vertex_buffer(0, buf.slice(..));
        }
        // Issue one draw per contiguous same-category run. That's how the
        // monolithic shader preserved cross-type z-order: one pipeline,
        // per-instance branching. The split pipelines enforce the same
        // ordering by switching pipelines between runs. Drawing each
        // pipeline over the full range (the old approach) lost ordering —
        // e.g. a section background at index 5 would paint over a shadow at
        // index 10 because the core pipeline ran after the shadow pipeline
        // regardless of instance order.
        for &(cat, start, end) in &ranges.runs {
            if start >= end {
                continue;
            }
            let pipeline = match (cat, overlay) {
                (SdfPipelineCategory::Shadow, false) => &pipelines.sdf_shadow,
                (SdfPipelineCategory::Shadow, true) => &pipelines.sdf_shadow_overlay,
                (SdfPipelineCategory::Notch, false) => &pipelines.sdf_notch,
                (SdfPipelineCategory::Notch, true) => &pipelines.sdf_notch_overlay,
                (SdfPipelineCategory::Sdf3D, false) => &pipelines.sdf_3d,
                (SdfPipelineCategory::Sdf3D, true) => &pipelines.sdf_3d_overlay,
                // Core and Text both ride the sdf_core pipeline — its
                // PRIM_TEXT branch samples the glyph atlas for prim_type=7.
                (SdfPipelineCategory::Core, false) | (SdfPipelineCategory::Text, false) => {
                    &pipelines.sdf_core
                }
                (SdfPipelineCategory::Core, true) | (SdfPipelineCategory::Text, true) => {
                    &pipelines.sdf_core_overlay
                }
            };
            render_pass.set_pipeline(pipeline);
            render_pass.draw(0..6, start..end);
        }
    }

    /// Walk the primitive slice and emit `(category, start_index, end_index)`
    /// tuples covering every contiguous run of same-category primitives.
    /// Drawing these in order preserves instance-index z-order across all
    /// pipeline categories.
    fn compute_category_runs(primitives: &[GpuPrimitive]) -> Vec<(SdfPipelineCategory, u32, u32)> {
        let mut runs = Vec::new();
        if primitives.is_empty() {
            return runs;
        }
        let mut start = 0u32;
        let mut cat = primitives[0].pipeline_category();
        for (i, p) in primitives.iter().enumerate().skip(1) {
            let c = p.pipeline_category();
            if c != cat {
                runs.push((cat, start, i as u32));
                start = i as u32;
                cat = c;
            }
        }
        runs.push((cat, start, primitives.len() as u32));
        runs
    }

    /// Issue draw calls for split SDF pipelines using MSAA pipeline variants.
    ///
    /// Used by `render_overlay_msaa` where a specific sample count is in play.
    /// When `vb_buffer` is `Some`, the instance vertex buffer is bound at slot 0
    /// (VERTEX_STORAGE fallback path).
    fn draw_split_sdf_msaa<'a>(
        render_pass: &mut wgpu::RenderPass<'a>,
        msaa: &'a MsaaPipelines,
        ranges: &SdfPrimitiveRanges,
        vb_buffer: Option<&'a wgpu::Buffer>,
    ) {
        if let Some(buf) = vb_buffer {
            render_pass.set_vertex_buffer(0, buf.slice(..));
        }
        // Full range for z-order preservation (same approach as draw_split_sdf)
        let total_start = [&ranges.core, &ranges.shadow, &ranges.sdf_3d, &ranges.notch]
            .iter()
            .filter(|r| !r.is_empty())
            .map(|r| r.start)
            .min();
        let total_end = [&ranges.core, &ranges.shadow, &ranges.sdf_3d, &ranges.notch]
            .iter()
            .filter(|r| !r.is_empty())
            .map(|r| r.end)
            .max();
        let full_range = match (total_start, total_end) {
            (Some(s), Some(e)) => s..e,
            _ => return,
        };
        // Per-run draws preserve cross-category z-order (same approach as
        // `draw_split_sdf`). `full_range` is now only used to short-circuit
        // empty batches.
        let _ = full_range;
        for &(cat, start, end) in &ranges.runs {
            if start >= end {
                continue;
            }
            let pipeline = match cat {
                SdfPipelineCategory::Shadow => &msaa.sdf_shadow,
                SdfPipelineCategory::Notch => &msaa.sdf_notch,
                SdfPipelineCategory::Sdf3D => &msaa.sdf_3d,
                // Core + Text both ride sdf_core (its PRIM_TEXT branch
                // samples the glyph atlas for prim_type=7).
                SdfPipelineCategory::Core | SdfPipelineCategory::Text => &msaa.sdf_core,
            };
            render_pass.set_pipeline(pipeline);
            render_pass.draw(0..6, start..end);
        }
    }

    /// Create a new renderer without a surface (for headless rendering)
    pub async fn new(config: RendererConfig) -> Result<Self, RendererError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: Self::preferred_backends(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| RendererError::AdapterNotFound)?;

        let required_limits = device_required_limits(&adapter);
        let config = apply_renderer_config_overrides(config, &required_limits);
        log_renderer_config(&config);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Blinc GPU Device"),
                required_features: requested_device_features(&adapter),
                required_limits,
                // MemoryUsage hint tells the driver to prefer lower memory over performance.
                // This helps reduce RSS on integrated GPUs (Apple Silicon) where GPU memory
                // is shared with CPU and counts against process memory.
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(RendererError::DeviceError)?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Default texture format for headless
        let texture_format = config
            .texture_format
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);

        Self::create_renderer(
            instance,
            adapter,
            device,
            queue,
            texture_format,
            config,
            (800, 600),
        )
    }

    /// Create a new renderer with a window surface
    pub async fn with_surface<W>(
        window: Arc<W>,
        config: RendererConfig,
    ) -> Result<(Self, wgpu::Surface<'static>), RendererError>
    where
        W: raw_window_handle::HasWindowHandle
            + raw_window_handle::HasDisplayHandle
            + Send
            + Sync
            + 'static,
    {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: Self::preferred_backends(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .map_err(RendererError::SurfaceError)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| RendererError::AdapterNotFound)?;

        let required_limits = device_required_limits(&adapter);
        let config = apply_renderer_config_overrides(config, &required_limits);
        log_renderer_config(&config);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Blinc GPU Device"),
                required_features: requested_device_features(&adapter),
                required_limits,
                // MemoryUsage hint tells the driver to prefer lower memory over performance.
                // This helps reduce RSS on integrated GPUs (Apple Silicon) where GPU memory
                // is shared with CPU and counts against process memory.
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(RendererError::DeviceError)?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_caps = surface.get_capabilities(&adapter);
        tracing::debug!("Surface capabilities - formats: {:?}", surface_caps.formats);
        tracing::debug!(
            "Surface capabilities - alpha modes: {:?}",
            surface_caps.alpha_modes
        );

        // Select texture format based on platform
        let texture_format = config.texture_format.unwrap_or_else(|| {
            // On macOS, prefer non-sRGB format to avoid automatic gamma correction
            // which causes colors to appear washed out. Other platforms may behave
            // differently, so we use sRGB there for now.
            #[cfg(target_os = "macos")]
            {
                surface_caps
                    .formats
                    .iter()
                    .find(|f| !f.is_srgb())
                    .copied()
                    .unwrap_or(surface_caps.formats[0])
            }
            #[cfg(not(target_os = "macos"))]
            {
                // On WebGL2 (GL adapter without storage buffers), prefer non-sRGB
                // to avoid double gamma correction — shaders output sRGB-encoded
                // colors directly, and an sRGB surface would apply gamma again.
                let prefer_non_srgb = adapter.limits().max_storage_buffers_per_shader_stage == 0;
                if prefer_non_srgb {
                    surface_caps
                        .formats
                        .iter()
                        .find(|f| !f.is_srgb())
                        .copied()
                        .unwrap_or(surface_caps.formats[0])
                } else {
                    surface_caps
                        .formats
                        .iter()
                        .find(|f| f.is_srgb())
                        .copied()
                        .unwrap_or(surface_caps.formats[0])
                }
            }
        });
        tracing::info!(
            "Selected texture format: {:?} (sRGB: {})",
            texture_format,
            texture_format.is_srgb()
        );

        let renderer = Self::create_renderer(
            instance,
            adapter,
            device,
            queue,
            texture_format,
            config,
            (800, 600),
        )?;

        // Force driver to compile deferred Vulkan pipelines now,
        // before the first surface present. No-op on macOS/Windows.
        renderer.pre_warm_pipelines();

        Ok((renderer, surface))
    }

    /// Create a new renderer with a `<canvas>` element on `wasm32`.
    ///
    /// Mirrors [`Self::with_surface`] but takes a
    /// [`web_sys::HtmlCanvasElement`] instead of a raw-window-handle
    /// type, because `HtmlCanvasElement` doesn't (and can't) implement
    /// `HasWindowHandle` / `HasDisplayHandle` — the browser exposes its
    /// surface through `wgpu::SurfaceTarget::Canvas` instead.
    ///
    /// The texture format is selected from the browser-reported surface
    /// capabilities, preferring an sRGB format. WebGPU's canonical
    /// preferred format on Chrome is `Bgra8UnormSrgb`, but Safari
    /// Technology Preview reports only `Rgba8UnormSrgb` — the
    /// `find(is_srgb)` lookup handles both.
    ///
    /// # Browser availability
    ///
    /// Requires WebGPU (Chrome ≥ 113, Edge ≥ 113, Safari Technology
    /// Preview, Firefox Nightly with the WebGPU flag). The `web` feature
    /// also enables the `webgl` backend so wgpu can fall back to WebGL2
    /// where WebGPU isn't available, but the fallback path will reject
    /// some Blinc shader features (storage buffers in particular).
    #[cfg(target_arch = "wasm32")]
    pub async fn with_canvas(
        canvas: web_sys::HtmlCanvasElement,
        config: RendererConfig,
    ) -> Result<(Self, wgpu::Surface<'static>), RendererError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: Self::preferred_backends(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(RendererError::SurfaceError)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| RendererError::AdapterNotFound)?;

        // Check that the adapter supports storage buffers in vertex
        // shaders — Blinc's SDF pipeline requires this (the primitives
        // buffer is `var<storage, read>` accessed from both vertex and
        // fragment stages). Skip on wasm32: the WebGPU spec guarantees
        // storage buffer support, but wgpu's downlevel report can be
        // wrong when the GL fallback adapter is selected (WebGL2 lacks
        // VERTEX_STORAGE, producing a false negative even though the
        // browser's WebGPU backend supports it).
        #[cfg(not(target_arch = "wasm32"))]
        {
            let downlevel = adapter.get_downlevel_capabilities();
            if !downlevel
                .flags
                .contains(wgpu::DownlevelFlags::VERTEX_STORAGE)
            {
                return Err(RendererError::ShaderError(
                    "GPU adapter does not support storage buffers in vertex shaders \
                     (VERTEX_STORAGE). Blinc requires this feature for its SDF \
                     rendering pipeline."
                        .to_string(),
                ));
            }
        }

        let required_limits = device_required_limits(&adapter);
        let config = apply_renderer_config_overrides(config, &required_limits);
        log_renderer_config(&config);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Blinc GPU Device (Web)"),
                required_features: requested_device_features(&adapter),
                required_limits,
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(RendererError::DeviceError)?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_caps = surface.get_capabilities(&adapter);
        tracing::debug!(
            "Web surface capabilities - formats: {:?}",
            surface_caps.formats
        );
        tracing::debug!(
            "Web surface capabilities - alpha modes: {:?}",
            surface_caps.alpha_modes
        );

        // On WebGPU (Chrome/Safari), prefer sRGB — the browser pipeline
        // expects sRGB output. On WebGL2 (GL adapter, no storage buffers),
        // prefer non-sRGB — shaders output sRGB-encoded colors directly,
        // and an sRGB surface would apply gamma encoding again (washed out).
        let is_gl_adapter = adapter.limits().max_storage_buffers_per_shader_stage == 0;
        let texture_format = config.texture_format.unwrap_or_else(|| {
            if is_gl_adapter {
                surface_caps
                    .formats
                    .iter()
                    .find(|f| !f.is_srgb())
                    .copied()
                    .unwrap_or(surface_caps.formats[0])
            } else {
                surface_caps
                    .formats
                    .iter()
                    .find(|f| f.is_srgb())
                    .copied()
                    .unwrap_or(surface_caps.formats[0])
            }
        });
        tracing::info!(
            "Web surface texture format: {:?} (sRGB: {}, GL adapter: {})",
            texture_format,
            texture_format.is_srgb(),
            is_gl_adapter
        );

        let renderer = Self::create_renderer(
            instance,
            adapter,
            device,
            queue,
            texture_format,
            config,
            (800, 600),
        )?;

        Ok((renderer, surface))
    }

    /// Create a new renderer with an existing wgpu instance and surface
    ///
    /// This is useful for platforms like Android where the surface is created
    /// from a native window handle before the renderer is initialized.
    pub async fn with_instance_and_surface(
        instance: wgpu::Instance,
        surface: &wgpu::Surface<'_>,
        config: RendererConfig,
    ) -> Result<Self, RendererError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| RendererError::AdapterNotFound)?;

        let required_limits = device_required_limits(&adapter);
        let config = apply_renderer_config_overrides(config, &required_limits);
        log_renderer_config(&config);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Blinc GPU Device"),
                required_features: requested_device_features(&adapter),
                required_limits,
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(RendererError::DeviceError)?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_caps = surface.get_capabilities(&adapter);
        tracing::debug!("Surface capabilities - formats: {:?}", surface_caps.formats);

        // Select texture format based on platform
        let texture_format = config.texture_format.unwrap_or_else(|| {
            // On Android, prefer non-sRGB format to match macOS behavior
            // Using sRGB causes colors to appear washed out because the GPU
            // applies automatic gamma correction
            surface_caps
                .formats
                .iter()
                .find(|f| !f.is_srgb())
                .copied()
                .unwrap_or(surface_caps.formats[0])
        });
        tracing::info!("Surface formats available: {:?}", surface_caps.formats);
        tracing::info!("Selected texture format: {:?}", texture_format);

        Self::create_renderer(
            instance,
            adapter,
            device,
            queue,
            texture_format,
            config,
            (800, 600),
        )
    }

    /// Pre-compile the eagerly-created render pipelines by issuing
    /// dummy draws against a throwaway texture.
    ///
    /// Vulkan drivers defer shader compilation to first-draw, so the
    /// first frame in a Linux app pays the cost of compiling 8+ SDF
    /// shaders + path/clear_quad sequentially on the main thread. On
    /// 8GB-RAM laptops the user reported this as "huge delay before
    /// the initial route renders". Pre-warming pushes that cost into
    /// renderer construction (before the window is visible), trading
    /// a slightly slower `with_surface` for a snappy first paint.
    ///
    /// Only pipelines whose required bind groups exist at this point
    /// are pre-warmed (SDF family + path + clear_quad). text_overlay,
    /// composite_overlay, and layer_composite need per-call bind
    /// groups created at first use, so they're left to JIT-compile on
    /// the first real frame — those are individually cheap. MSAA
    /// variants are created lazily for the active sample_count and
    /// aren't pre-warmed here either.
    ///
    /// Opt out with `BLINC_PIPELINE_PREWARM=0`. No-op on non-Linux
    /// (Metal/DX12 don't defer compilation this way).
    #[cfg(target_os = "linux")]
    fn pre_warm_pipelines(&self) {
        if std::env::var("BLINC_PIPELINE_PREWARM").as_deref() == Ok("0") {
            tracing::debug!("Pipeline pre-warm disabled via BLINC_PIPELINE_PREWARM=0");
            return;
        }

        let start = std::time::Instant::now();

        let throwaway = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Pipeline Pre-Warm Throwaway"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = throwaway.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Pipeline Pre-Warm Encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Pipeline Pre-Warm Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Discard,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // clear_quad — no bind group required.
            pass.set_pipeline(&self.pipelines.clear_quad);
            pass.draw(0..3, 0..1);

            // SDF family shares `bind_groups.sdf` at slot 0.
            pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            for sdf_pipeline in [
                &self.pipelines.sdf_core,
                &self.pipelines.sdf_shadow,
                &self.pipelines.sdf_3d,
                &self.pipelines.sdf_notch,
                &self.pipelines.sdf_core_overlay,
                &self.pipelines.sdf_shadow_overlay,
                &self.pipelines.sdf_3d_overlay,
                &self.pipelines.sdf_notch_overlay,
            ] {
                pass.set_pipeline(sdf_pipeline);
                pass.draw(0..6, 0..1);
            }

            // Path family shares `bind_groups.path` at slot 0.
            pass.set_bind_group(0, &self.bind_groups.path, &[]);
            for path_pipeline in [&self.pipelines.path, &self.pipelines.path_overlay] {
                pass.set_pipeline(path_pipeline);
                pass.draw(0..3, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Block until the GPU has actually finished compiling and
        // running these. Without this the work stays queued and the
        // first real frame still pays the compile cost.
        let _ = self.device.poll(wgpu::PollType::Wait);

        tracing::info!(
            "Vulkan pipeline pre-warm complete in {:.1} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }

    /// No-op pre-warm for non-Linux targets — Metal (macOS),
    /// DX12 (Windows), and WebGPU compile shaders eagerly at
    /// pipeline creation, so there's nothing to warm up.
    #[cfg(not(target_os = "linux"))]
    fn pre_warm_pipelines(&self) {}

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
            source: wgpu::ShaderSource::Wgsl(PATH_SHADER.into()),
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

        // The monolithic SDF_SHADER is no longer compiled — split pipelines
        // handle all primitive types. Use core shader as stand-in for the
        // dead-code monolithic pipeline fields in the Pipelines struct.
        let sdf_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Shader (stand-in — split pipelines active)"),
            source: wgpu::ShaderSource::Wgsl(sdf_core_source.into()),
        });

        let sdf_core_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Core Shader"),
            source: wgpu::ShaderSource::Wgsl(sdf_core_source.into()),
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

        // Create pipelines (core only — effect pipelines are lazy)
        let pipelines = Self::create_pipelines(
            &device,
            &bind_group_layouts,
            &sdf_shader,
            &sdf_core_shader,
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
        );

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
            cached_msaa: None,
            cached_glass: None,
            cached_text: None,
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
            scene_copy_texture: None,
            has_vertex_storage,
            has_storage_buffers,
            has_texture_compression_bc,
            static_layer: None,
        })
    }

    fn create_bind_group_layouts_with_flags(
        device: &wgpu::Device,
        has_vertex_storage: bool,
        has_storage_buffers: bool,
    ) -> BindGroupLayouts {
        // When VERTEX_STORAGE is available, the primitives storage buffer
        // is visible to both vertex and fragment stages. Otherwise, only
        // the fragment stage reads it — the vertex shader gets its data
        // from an instance-stepped vertex buffer instead.
        let primitives_visibility = if has_vertex_storage {
            wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT
        } else {
            wgpu::ShaderStages::FRAGMENT
        };

        // Binding 1 & 5: Storage buffers normally; data textures when no storage support
        let binding_1_entry = if has_storage_buffers {
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: primitives_visibility,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }
        } else {
            // DT mode: primitive data comes from an Rgba32Float texture.
            // VERTEX | FRAGMENT visibility needed because WGSL module-scope
            // bindings are validated against all entry points in the module,
            // even if only fs_main reads the texture. Texture bindings don't
            // require VERTEX_STORAGE (that flag only applies to storage buffers).
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }
        };

        let binding_5_entry = if has_storage_buffers {
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                // VERTEX as well as FRAGMENT: `PRIM_MESH` routes
                // tessellated path fills through this pipeline and
                // has the vertex shader pull each triangle's three
                // corners from `aux_data` at the primitive's
                // `border.z` offset so hardware rasterises the
                // real triangle (instead of a per-pixel point-in-
                // triangle walk over an AABB-covering quad). Without
                // VERTEX visibility wgpu rejects the pipeline
                // layout because the shader reads a binding the
                // layout didn't authorise for that stage.
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }
        } else {
            // DT mode: aux data comes from an Rgba32Float texture
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }
        };

        // SDF bind group layout (includes glyph atlas for unified text rendering)
        let sdf = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SDF Bind Group Layout"),
            entries: &[
                // Uniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Binding 1: Primitives (storage buffer or data texture)
                binding_1_entry,
                // Glyph atlas texture (grayscale text)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Glyph sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Color glyph atlas texture (emoji)
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Binding 5: Auxiliary data (storage buffer or data texture)
                binding_5_entry,
            ],
        });

        // Glass bind group layout
        let glass = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Glass Bind Group Layout"),
            entries: &[
                // Uniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Glass primitives: storage buffer (normal) or texture (WebGL2 DT fallback).
                // VERTEX | FRAGMENT in both modes — DT shader declares binding at module
                // scope, wgpu validates against all entry points.
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: if has_storage_buffers {
                        wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        }
                    } else {
                        wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        }
                    },
                    count: None,
                },
                // Backdrop texture
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Backdrop sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Text bind group layout
        let text = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Text Bind Group Layout"),
            entries: &[
                // Uniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Glyphs: storage buffer (normal) or texture (WebGL2 DT fallback)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: if has_storage_buffers {
                        wgpu::ShaderStages::VERTEX
                    } else {
                        // DT mode: TEXT_DT_SHADER reads glyph_data texture in vs_main
                        wgpu::ShaderStages::VERTEX
                    },
                    ty: if has_storage_buffers {
                        wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        }
                    } else {
                        wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        }
                    },
                    count: None,
                },
                // Glyph atlas texture
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Glyph atlas sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Color glyph atlas texture (for emoji)
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        // Composite bind group layout
        let composite = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Composite Bind Group Layout"),
            entries: &[
                // Uniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Source texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Source sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Path bind group layout (uniforms + gradient texture + image texture + backdrop for glass)
        let path = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Path Bind Group Layout"),
            entries: &[
                // Uniforms (viewport_size, transform, opacity, clip, glass params, etc.)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Gradient texture (1D texture for multi-stop gradients)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D1,
                        multisampled: false,
                    },
                    count: None,
                },
                // Gradient sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Image texture (2D texture for image brush)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Image sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Backdrop texture (2D texture for glass effect)
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Backdrop sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Aux data storage buffer — carries polygon clip vertices
                // packed 2-per-vec4. Shared with the SDF pipeline's
                // binding(5) so tessellated path fills honour the same
                // `ClipShape::Polygon` clips (Lottie track mattes are
                // built this way). Re-bound via `recreate_path_bind_group`
                // whenever the aux buffer gets resized.
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Layer composite bind group layout (for compositing offscreen layers)
        let layer_composite = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Layer Composite Bind Group Layout"),
            entries: &[
                // Uniforms (source_rect, dest_rect, viewport_size, opacity, blend_mode)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Layer texture (source)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Layer sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Destination texture (for blend modes)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Destination sampler (for blend modes)
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Blur effect bind group layout
        let blur = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Blur Effect Bind Group Layout"),
            entries: &[
                // BlurUniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Input texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Input sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Color matrix effect bind group layout
        let color_matrix = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Color Matrix Effect Bind Group Layout"),
            entries: &[
                // ColorMatrixUniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Input texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Input sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Drop shadow effect bind group layout
        let drop_shadow = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Drop Shadow Effect Bind Group Layout"),
            entries: &[
                // DropShadowUniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Blurred input texture (for shadow alpha)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Input sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Original (unblurred) texture (for compositing)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        // Glow effect bind group layout
        let glow = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Glow Effect Bind Group Layout"),
            entries: &[
                // GlowUniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Source texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Input sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Mask image effect bind group layout
        let mask_image = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Mask Image Effect Bind Group Layout"),
            entries: &[
                // MaskUniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Input (element) texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Input sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Mask texture
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Mask sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        BindGroupLayouts {
            sdf,
            glass,
            text,
            composite,
            path,
            layer_composite,
            blur,
            color_matrix,
            drop_shadow,
            glow,
            mask_image,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn create_pipelines(
        device: &wgpu::Device,
        layouts: &BindGroupLayouts,
        sdf_shader: &wgpu::ShaderModule,
        sdf_core_shader: &wgpu::ShaderModule,
        sdf_shadow_shader: &wgpu::ShaderModule,
        sdf_3d_shader: &wgpu::ShaderModule,
        sdf_notch_shader: &wgpu::ShaderModule,
        text_shader: &wgpu::ShaderModule,
        composite_shader: &wgpu::ShaderModule,
        path_shader: &wgpu::ShaderModule,
        layer_composite_shader: &wgpu::ShaderModule,
        clear_quad_shader: &wgpu::ShaderModule,
        texture_format: wgpu::TextureFormat,
        sample_count: u32,
        has_vertex_storage: bool,
    ) -> Pipelines {
        let blend_state = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let color_targets = &[Some(wgpu::ColorTargetState {
            format: texture_format,
            blend: Some(blend_state),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let primitive_state = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        };

        let multisample_state = wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        // SDF pipeline
        let sdf_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SDF Pipeline Layout"),
            bind_group_layouts: &[&layouts.sdf],
            push_constant_ranges: &[],
        });

        // When VERTEX_STORAGE is unavailable, SDF vertex shaders read from an
        // instance-stepped vertex buffer instead of the storage buffer.
        let sdf_vb_buffers: &[wgpu::VertexBufferLayout<'_>] = if has_vertex_storage {
            &[]
        } else {
            &[SdfVertexInstance::LAYOUT]
        };

        let sdf = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SDF Pipeline"),
            layout: Some(&sdf_layout),
            vertex: wgpu::VertexState {
                module: sdf_shader,
                entry_point: Some("vs_main"),
                buffers: sdf_vb_buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: sdf_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        // Overlay pipelines use sample_count=1 for rendering on resolved textures
        let overlay_multisample_state = wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        let sdf_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SDF Overlay Pipeline"),
            layout: Some(&sdf_layout),
            vertex: wgpu::VertexState {
                module: sdf_shader,
                entry_point: Some("vs_main"),
                buffers: sdf_vb_buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: sdf_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: None,
        });

        // --- Split SDF pipelines (share sdf_layout, same blend/primitive state) ---

        // Helper closure to create an SDF pipeline pair (MSAA + overlay) from a shader module
        let make_sdf_pipeline_pair = |shader: &wgpu::ShaderModule,
                                      label: &str|
         -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
            let msaa = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&sdf_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    buffers: sdf_vb_buffers,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: color_targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: primitive_state,
                depth_stencil: None,
                multisample: multisample_state,
                multiview: None,
                cache: None,
            });
            let overlay_label = format!("{label} Overlay");
            let overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&overlay_label),
                layout: Some(&sdf_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    buffers: sdf_vb_buffers,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: color_targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: primitive_state,
                depth_stencil: None,
                multisample: overlay_multisample_state,
                multiview: None,
                cache: None,
            });
            (msaa, overlay)
        };

        let (sdf_core, sdf_core_overlay) =
            make_sdf_pipeline_pair(sdf_core_shader, "SDF Core Pipeline");
        let (sdf_shadow, sdf_shadow_overlay) =
            make_sdf_pipeline_pair(sdf_shadow_shader, "SDF Shadow Pipeline");
        let (sdf_3d, sdf_3d_overlay) = make_sdf_pipeline_pair(sdf_3d_shader, "SDF 3D Pipeline");
        let (sdf_notch, sdf_notch_overlay) =
            make_sdf_pipeline_pair(sdf_notch_shader, "SDF Notch Pipeline");

        // Text pipeline
        let text_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Text Pipeline Layout"),
            bind_group_layouts: &[&layouts.text],
            push_constant_ranges: &[],
        });

        // TEXT_DT_SHADER has its own vs_main that reads from a glyph data texture
        // (no VB instance attributes needed — unlike SDF DT which uses VB + DT).
        let text_vb_buffers: &[wgpu::VertexBufferLayout<'_>] = &[];

        let text = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text Pipeline"),
            layout: Some(&text_layout),
            vertex: wgpu::VertexState {
                module: text_shader,
                entry_point: Some("vs_main"),
                buffers: text_vb_buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: text_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        // Text overlay pipeline - uses sample_count=1 for rendering on resolved textures
        let text_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text Overlay Pipeline"),
            layout: Some(&text_layout),
            vertex: wgpu::VertexState {
                module: text_shader,
                entry_point: Some("vs_main"),
                buffers: text_vb_buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: text_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: None,
        });

        // Composite pipeline
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Composite Pipeline Layout"),
            bind_group_layouts: &[&layouts.composite],
            push_constant_ranges: &[],
        });

        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Composite Pipeline"),
            layout: Some(&composite_layout),
            vertex: wgpu::VertexState {
                module: composite_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: composite_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        // Composite overlay pipeline - single-sampled for blending onto resolved textures
        let composite_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Composite Overlay Pipeline"),
            layout: Some(&composite_layout),
            vertex: wgpu::VertexState {
                module: composite_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: composite_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: None,
        });

        // Path pipeline - uses vertex buffers for tessellated geometry
        let path_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Path Pipeline Layout"),
            bind_group_layouts: &[&layouts.path],
            push_constant_ranges: &[],
        });

        // Vertex buffer layout for PathVertex
        // PathVertex layout (80 bytes total):
        //   position: [f32; 2]       - 8 bytes, offset 0
        //   color: [f32; 4]          - 16 bytes, offset 8
        //   end_color: [f32; 4]      - 16 bytes, offset 24
        //   uv: [f32; 2]             - 8 bytes, offset 40
        //   gradient_params: [f32;4] - 16 bytes, offset 48
        //   gradient_type: u32       - 4 bytes, offset 64
        //   edge_distance: f32       - 4 bytes, offset 68
        //   clip_bounds: [f32;4]     - 16 bytes, offset 72
        //   clip_radius: [f32;4]     - 16 bytes, offset 88
        //   clip_type: u32           - 4 bytes, offset 104
        //   _padding: [u32; 3]       - 12 bytes, offset 108
        let path_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PathVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position: vec2<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                // color: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 8,
                    shader_location: 1,
                },
                // end_color: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 24,
                    shader_location: 2,
                },
                // uv: vec2<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 40,
                    shader_location: 3,
                },
                // gradient_params: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 4,
                },
                // gradient_type: u32
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 64,
                    shader_location: 5,
                },
                // edge_distance: f32 (for anti-aliasing)
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 68,
                    shader_location: 6,
                },
                // clip_bounds: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 72,
                    shader_location: 7,
                },
                // clip_radius: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 88,
                    shader_location: 8,
                },
                // clip_type: u32
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 104,
                    shader_location: 9,
                },
            ],
        };

        let path = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Path Pipeline"),
            layout: Some(&path_layout),
            vertex: wgpu::VertexState {
                module: path_shader,
                entry_point: Some("vs_main"),
                buffers: std::slice::from_ref(&path_vertex_layout),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: path_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        // Path overlay pipeline - uses sample_count=1 for rendering on resolved textures
        let path_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Path Overlay Pipeline"),
            layout: Some(&path_layout),
            vertex: wgpu::VertexState {
                module: path_shader,
                entry_point: Some("vs_main"),
                buffers: &[path_vertex_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: path_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: None,
        });

        // Layer composite pipeline - for compositing offscreen layers with blend modes
        let layer_composite_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Layer Composite Pipeline Layout"),
                bind_group_layouts: &[&layouts.layer_composite],
                push_constant_ranges: &[],
            });

        // Use premultiplied alpha blending for layer composition
        let premultiplied_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let layer_composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Layer Composite Pipeline"),
            layout: Some(&layer_composite_layout),
            vertex: wgpu::VertexState {
                module: layer_composite_shader,
                entry_point: Some("vs_main"),
                buffers: &[], // No vertex buffers - quad generated in shader
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: layer_composite_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
                    blend: Some(premultiplied_blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: overlay_multisample_state, // 1x sampled - layers are resolved
            multiview: None,
            cache: None,
        });

        // Compositor v2 scissored clear. No bindings, no vertex
        // buffer — the shader generates a fullscreen triangle from
        // `vertex_index`. REPLACE blend so the fragment's
        // `(0,0,0,0)` output fully overwrites the attachment inside
        // the active scissor rect.
        let clear_quad_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Clear Quad Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let clear_quad = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Clear Quad Pipeline"),
            layout: Some(&clear_quad_layout),
            vertex: wgpu::VertexState {
                module: clear_quad_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: clear_quad_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
                    // REPLACE blend: fragment output replaces dest
                    // verbatim. No multiplicative compositing — we
                    // want the cleared pixels to be exactly
                    // (0,0,0,0), not blended with whatever was there.
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: None,
        });

        Pipelines {
            sdf,
            sdf_overlay,
            sdf_core,
            sdf_shadow,
            sdf_3d,
            sdf_notch,
            sdf_core_overlay,
            sdf_shadow_overlay,
            sdf_3d_overlay,
            sdf_notch_overlay,
            text,
            text_overlay,
            composite,
            composite_overlay,
            path,
            path_overlay,
            layer_composite,
            clear_quad,
        }
    }

    fn create_buffers(
        device: &wgpu::Device,
        config: &RendererConfig,
        has_storage_buffers: bool,
    ) -> Buffers {
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniforms Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Storage buffers are always created (even in DT mode they are needed by
        // non-SDF pipelines like glass). In DT mode the SDF bind group uses data
        // textures instead, but these buffers remain for other uses.
        let primitives = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Primitives Buffer"),
            size: (std::mem::size_of::<GpuPrimitive>() * config.max_primitives) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let glass_primitives = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Glass Primitives Buffer"),
            size: (std::mem::size_of::<GpuGlassPrimitive>() * config.max_glass_primitives) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let glass_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Glass Uniforms Buffer"),
            size: std::mem::size_of::<GlassUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let glyphs = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Glyphs Buffer"),
            size: (std::mem::size_of::<GpuGlyph>() * config.max_glyphs) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let path_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Path Uniforms Buffer"),
            size: std::mem::size_of::<PathUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Auxiliary data buffer for variable-length per-primitive data
        // Initial size: 1 vec4 (minimum for valid binding, will be recreated if needed)
        let aux_data = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Aux Data Buffer"),
            size: 16, // 1 vec4<f32> minimum
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create data textures for DT (Tier 3) fallback when no storage buffers
        let (
            prim_data_texture,
            prim_data_view,
            aux_data_texture,
            aux_data_view,
            aux_data_texture_height,
            glyph_data_texture,
            glyph_data_view,
        ) = if !has_storage_buffers {
            // Primitive data texture: width=23 (one texel per vec4 field), height=max_primitives
            let prim_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Primitive Data Texture"),
                size: wgpu::Extent3d {
                    width: 23,
                    height: config.max_primitives as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let prim_view = prim_tex.create_view(&wgpu::TextureViewDescriptor::default());

            // Aux data texture: width=1024, height=1 initially (resized on demand)
            let aux_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Aux Data Texture"),
                size: wgpu::Extent3d {
                    width: 1024,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let aux_view = aux_tex.create_view(&wgpu::TextureViewDescriptor::default());

            // Glyph data texture: width=6 (one texel per vec4 field), height=max_glyphs
            let glyph_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Glyph Data Texture"),
                size: wgpu::Extent3d {
                    width: 6,
                    height: config.max_glyphs as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let glyph_view = glyph_tex.create_view(&wgpu::TextureViewDescriptor::default());

            (
                Some(prim_tex),
                Some(prim_view),
                Some(aux_tex),
                Some(aux_view),
                1u32,
                Some(glyph_tex),
                Some(glyph_view),
            )
        } else {
            (None, None, None, None, 0u32, None, None)
        };

        Buffers {
            uniforms,
            primitives,
            glass_primitives,
            glass_uniforms,
            glyphs,
            path_uniforms,
            path_vertices: None,
            path_indices: None,
            blur_uniforms_pool: None,
            drop_shadow_uniforms: None,
            glow_uniforms: None,
            color_matrix_uniforms: None,
            aux_data,
            sdf_vertex_instances: None,
            prim_data_texture,
            prim_data_view,
            aux_data_texture,
            aux_data_view,
            aux_data_texture_height,
            glyph_data_texture,
            glyph_data_view,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn create_bind_groups(
        device: &wgpu::Device,
        layouts: &BindGroupLayouts,
        buffers: &Buffers,
        glyph_atlas_view: &wgpu::TextureView,
        color_glyph_atlas_view: &wgpu::TextureView,
        glyph_sampler: &wgpu::Sampler,
        gradient_texture_cache: &GradientTextureCache,
        path_image_view: &wgpu::TextureView,
        path_image_sampler: &wgpu::Sampler,
        has_storage_buffers: bool,
    ) -> BindGroups {
        // Binding 1: primitives (storage buffer or data texture)
        let binding_1 = if has_storage_buffers {
            wgpu::BindGroupEntry {
                binding: 1,
                resource: buffers.primitives.as_entire_binding(),
            }
        } else {
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    buffers
                        .prim_data_view
                        .as_ref()
                        .expect("DT mode requires prim_data_view"),
                ),
            }
        };

        // Binding 5: aux data (storage buffer or data texture)
        let binding_5 = if has_storage_buffers {
            wgpu::BindGroupEntry {
                binding: 5,
                resource: buffers.aux_data.as_entire_binding(),
            }
        } else {
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(
                    buffers
                        .aux_data_view
                        .as_ref()
                        .expect("DT mode requires aux_data_view"),
                ),
            }
        };

        let sdf = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF Bind Group"),
            layout: &layouts.sdf,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers.uniforms.as_entire_binding(),
                },
                binding_1,
                // Glyph atlas texture (binding 2)
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(glyph_atlas_view),
                },
                // Glyph sampler (binding 3)
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(glyph_sampler),
                },
                // Color glyph atlas texture (binding 4)
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(color_glyph_atlas_view),
                },
                binding_5,
            ],
        });

        // Path bind group (with gradient texture, image texture, and backdrop for glass)
        let path = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Path Bind Group"),
            layout: &layouts.path,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers.path_uniforms.as_entire_binding(),
                },
                // Gradient texture (binding 1)
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&gradient_texture_cache.view),
                },
                // Gradient sampler (binding 2)
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&gradient_texture_cache.sampler),
                },
                // Image texture (binding 3)
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(path_image_view),
                },
                // Image sampler (binding 4)
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(path_image_sampler),
                },
                // Backdrop texture (binding 5) - uses placeholder, will be replaced when glass is enabled
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(path_image_view),
                },
                // Backdrop sampler (binding 6)
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(path_image_sampler),
                },
                // Aux data (binding 7) — shared with the SDF pipeline.
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: buffers.aux_data.as_entire_binding(),
                },
            ],
        });

        // Glass bind group will be created when we have a backdrop texture
        BindGroups {
            sdf,
            glass: None,
            path,
        }
    }

    /// Create MSAA-specific pipelines for a given sample count
    fn create_msaa_pipelines(
        device: &wgpu::Device,
        layouts: &BindGroupLayouts,
        texture_format: wgpu::TextureFormat,
        sample_count: u32,
        has_vertex_storage: bool,
        has_storage_buffers: bool,
    ) -> MsaaPipelines {
        let blend_state = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let color_targets = &[Some(wgpu::ColorTargetState {
            format: texture_format,
            blend: Some(blend_state),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let primitive_state = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        };

        let multisample_state = wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        // Monolithic stand-in (MSAA) — uses core shader to avoid compiling
        // the full SDF_SHADER which exceeds PowerVR's shader compiler limit.
        let msaa_core_source = if !has_storage_buffers {
            SDF_CORE_DT_SHADER
        } else if !has_vertex_storage {
            SDF_CORE_VB_SHADER
        } else {
            SDF_CORE_SHADER
        };
        let sdf_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Shader (MSAA stand-in)"),
            source: wgpu::ShaderSource::Wgsl(msaa_core_source.into()),
        });

        let sdf_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SDF Pipeline Layout (MSAA)"),
            bind_group_layouts: &[&layouts.sdf],
            push_constant_ranges: &[],
        });

        let sdf = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SDF Pipeline (MSAA)"),
            layout: Some(&sdf_layout),
            vertex: wgpu::VertexState {
                module: &sdf_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sdf_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        // Split SDF shader modules (MSAA)
        let sdf_vb_buffers: &[wgpu::VertexBufferLayout<'_>] = if has_vertex_storage {
            &[]
        } else {
            &[SdfVertexInstance::LAYOUT]
        };
        let make_msaa_sdf_pipeline = |source: &str, label: &str| -> wgpu::RenderPipeline {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&sdf_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: sdf_vb_buffers,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: color_targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: primitive_state,
                depth_stencil: None,
                multisample: multisample_state,
                multiview: None,
                cache: None,
            })
        };

        let (msaa_core_src, msaa_shadow_src, msaa_3d_src, msaa_notch_src) = if !has_storage_buffers
        {
            (
                SDF_CORE_DT_SHADER,
                SDF_SHADOW_DT_SHADER,
                SDF_3D_DT_SHADER,
                SDF_NOTCH_DT_SHADER,
            )
        } else if !has_vertex_storage {
            (
                SDF_CORE_VB_SHADER,
                SDF_SHADOW_VB_SHADER,
                SDF_3D_VB_SHADER,
                SDF_NOTCH_VB_SHADER,
            )
        } else {
            (
                SDF_CORE_SHADER,
                SDF_SHADOW_SHADER,
                SDF_3D_SHADER,
                SDF_NOTCH_SHADER,
            )
        };

        let sdf_core = make_msaa_sdf_pipeline(msaa_core_src, "SDF Core Pipeline (MSAA)");
        let sdf_shadow = make_msaa_sdf_pipeline(msaa_shadow_src, "SDF Shadow Pipeline (MSAA)");
        let sdf_3d = make_msaa_sdf_pipeline(msaa_3d_src, "SDF 3D Pipeline (MSAA)");
        let sdf_notch = make_msaa_sdf_pipeline(msaa_notch_src, "SDF Notch Pipeline (MSAA)");

        // Create path shader
        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Path Shader (MSAA)"),
            source: wgpu::ShaderSource::Wgsl(PATH_SHADER.into()),
        });

        let path_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Path Pipeline Layout (MSAA)"),
            bind_group_layouts: &[&layouts.path],
            push_constant_ranges: &[],
        });

        // PathVertex layout — see PathVertex struct in path.rs for offset rationale
        let path_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PathVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 8,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 24,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 40,
                    shader_location: 3,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 4,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 64,
                    shader_location: 5,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 68,
                    shader_location: 6,
                },
                // clip_bounds: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 72,
                    shader_location: 7,
                },
                // clip_radius: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 88,
                    shader_location: 8,
                },
                // clip_type: u32
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 104,
                    shader_location: 9,
                },
            ],
        };

        let path = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Path Pipeline (MSAA)"),
            layout: Some(&path_layout),
            vertex: wgpu::VertexState {
                module: &path_shader,
                entry_point: Some("vs_main"),
                buffers: &[path_vertex_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &path_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        MsaaPipelines {
            sdf,
            sdf_core,
            sdf_shadow,
            sdf_3d,
            sdf_notch,
            path,
            sample_count,
        }
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

    /// Create a new surface for an additional window.
    ///
    /// Uses the existing wgpu Instance to create a surface that can be
    /// configured and rendered to using the shared device and queue.
    /// This is used for multi-window support.
    pub fn create_surface<W>(&self, window: Arc<W>) -> Result<wgpu::Surface<'static>, RendererError>
    where
        W: raw_window_handle::HasWindowHandle
            + raw_window_handle::HasDisplayHandle
            + Send
            + Sync
            + 'static,
    {
        self.instance
            .create_surface(wgpu::SurfaceTarget::from(window))
            .map_err(RendererError::SurfaceError)
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
        let _ = self.device.poll(wgpu::PollType::Wait);
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

    /// Get a mutable reference to the @flow pipeline cache
    pub fn flow_pipeline_cache(&mut self) -> &mut crate::flow_pipeline::FlowPipelineCache {
        &mut self.flow_pipeline_cache
    }

    /// Render a @flow fragment shader into a target texture.
    ///
    /// Compiles the flow on first use, updates uniforms, and draws a fullscreen quad.
    /// If `viewport` is Some([x, y, w, h]), the quad is scoped to that region in pixels.
    /// Returns false if the flow is not found or compilation failed.
    /// Ensure the scene copy texture exists, matches viewport size, and is up-to-date.
    ///
    /// Called once per frame before rendering any flows that use `sample_scene()`.
    /// Returns the scene texture view, or None if the copy failed.
    fn ensure_scene_copy(&mut self) -> Option<&wgpu::TextureView> {
        let (tw, th) = self.viewport_size;
        if tw == 0 || th == 0 {
            return None;
        }

        // Recreate texture on viewport resize
        let needs_recreate = match &self.scene_copy_texture {
            Some((_, _, w, h)) => *w != tw || *h != th,
            None => true,
        };
        if needs_recreate {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Flow Scene Copy Texture"),
                size: wgpu::Extent3d {
                    width: tw,
                    height: th,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.texture_format,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.scene_copy_texture = Some((tex, view, tw, th));
            // Scene texture changed — invalidate bind groups that reference it
            self.flow_pipeline_cache.invalidate_scene_bind_groups();
        }

        // Copy current render target → scene copy texture (single copy per frame)
        if let Some((scene_tex, _, _, _)) = &self.scene_copy_texture {
            if let Some(tex_ptr) = self.blend_target_ptr {
                let src_tex = unsafe { &*tex_ptr };
                let mut copy_encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Flow Scene Copy Encoder"),
                        });
                copy_encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: src_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: scene_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: tw,
                        height: th,
                        depth_or_array_layers: 1,
                    },
                );
                self.queue.submit(std::iter::once(copy_encoder.finish()));
            }
        }

        self.scene_copy_texture.as_ref().map(|(_, v, _, _)| v)
    }

    pub fn render_flow(
        &mut self,
        target: &wgpu::TextureView,
        flow_name: &str,
        uniforms: &crate::flow_pipeline::FlowUniformData,
        viewport: Option<[f32; 4]>,
        clip_rect: Option<[f32; 4]>,
    ) -> bool {
        // If this flow uses sample_scene(), ensure scene copy is ready
        let scene_view_owned: Option<*const wgpu::TextureView> =
            if self.flow_pipeline_cache.needs_scene_texture(flow_name) {
                self.ensure_scene_copy().map(|v| v as *const _)
            } else {
                None
            };
        // SAFETY: scene_copy_texture is owned by self and lives for the duration of this call
        let scene_view = scene_view_owned.map(|ptr| unsafe { &*ptr });

        if !self
            .flow_pipeline_cache
            .prepare_render(&self.queue, flow_name, uniforms, scene_view)
        {
            return false;
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Flow Render Encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Flow Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Preserve existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Scope the flow quad to the element's bounds via viewport
            // + scissor. The viewport is always set to the element's
            // full bounds (so the shader sees consistent UVs across
            // the quad), but the scissor additionally intersects with
            // the active clip stack — without that intersection, a
            // flow element scrolled out of its container or near a
            // window edge would visibly leak past the clip boundary
            // (see `gotcha_flow_render_ignores_active_clips.md`).
            if let Some([x, y, w, h]) = viewport {
                let (tw, th) = self.viewport_size;
                let tw = tw as f32;
                let th = th as f32;
                let cx = x.max(0.0);
                let cy = y.max(0.0);
                let cw = (w - (cx - x)).min(tw - cx).max(1.0);
                let ch = (h - (cy - y)).min(th - cy).max(1.0);
                if cx < tw && cy < th {
                    pass.set_viewport(cx, cy, cw, ch, 0.0, 1.0);
                    // Intersect with the active clip (scroll, overflow,
                    // overlay) when present, then clamp to the
                    // render-target bounds.
                    let (mut sx, mut sy, mut sw, mut sh) = (cx, cy, cw, ch);
                    if let Some([ax, ay, aw, ah]) = clip_rect {
                        let x0 = sx.max(ax);
                        let y0 = sy.max(ay);
                        let x1 = (sx + sw).min(ax + aw);
                        let y1 = (sy + sh).min(ay + ah);
                        sx = x0;
                        sy = y0;
                        sw = (x1 - x0).max(0.0);
                        sh = (y1 - y0).max(0.0);
                    }
                    // Re-clamp to target bounds after the clip
                    // intersection so a clip that overhangs the
                    // viewport doesn't trip wgpu validation.
                    let sx = sx.max(0.0);
                    let sy = sy.max(0.0);
                    let sw = sw.min(tw - sx).max(0.0);
                    let sh = sh.min(th - sy).max(0.0);
                    if sw > 0.0 && sh > 0.0 {
                        pass.set_scissor_rect(sx as u32, sy as u32, sw as u32, sh as u32);
                    } else {
                        // Empty intersection — skip the draw entirely
                        // by setting a 1×1 scissor outside the
                        // viewport so the pipeline still runs without
                        // touching pixels.
                        pass.set_scissor_rect(0, 0, 1, 1);
                    }
                }
            }

            self.flow_pipeline_cache
                .render_fragment(&mut pass, flow_name);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        true
    }

    /// Render a batch of primitives to a texture view
    /// Render primitives with transparent background (default)
    pub fn render(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        self.render_with_clear(target, batch, [0.0, 0.0, 0.0, 0.0]);
    }

    /// Render primitives at a specific viewport size (for reduced-resolution rendering)
    ///
    /// Used for glass backdrop rendering at half resolution.
    pub fn render_at_size(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
        width: u32,
        height: u32,
    ) {
        // Temporarily override viewport size for this render
        let original_size = self.viewport_size;
        self.viewport_size = (width, height);
        self.render_with_clear(target, batch, clear_color);
        self.viewport_size = original_size;
    }

    /// Render primitives with a specified clear color
    /// Ensure the layer-compositor's static-cache texture exists at
    /// the given dimensions and matches the surface format. Allocates
    /// on first use; reallocates on viewport resize. Marks the new
    /// texture invalid so the next `render_with_clear` knows it has
    /// to repaint the cache from scratch.
    pub fn ensure_static_layer(&mut self, width: u32, height: u32) {
        if let Some(layer) = &self.static_layer {
            if layer.width == width && layer.height == height {
                return;
            }
        }
        if width == 0 || height == 0 {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blinc-static-layer"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture_format,
            // RENDER_ATTACHMENT so we can render into it; COPY_SRC so
            // we can `copy_texture_to_texture` it onto the surface
            // each compositor frame.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("blinc-static-layer-view"),
            ..Default::default()
        });
        self.static_layer = Some(StaticLayer {
            texture,
            view,
            width,
            height,
            valid: false,
        });
    }

    /// Mark the static-cache texture invalid. Called when the
    /// composition source changes (rebuild, layout change, CSS state
    /// flip, scroll, motion-binding tick) — the next compositor
    /// frame will repaint the cache before using it.
    pub fn invalidate_static_layer(&mut self) {
        if let Some(layer) = self.static_layer.as_mut() {
            layer.valid = false;
        }
    }

    /// `true` if a static-cache texture exists and was painted into
    /// since the last invalidation. The compositor's fast path
    /// blits the cache when this is true; otherwise it falls back to
    /// rendering the cache afresh.
    pub fn static_layer_valid(&self) -> bool {
        self.static_layer.as_ref().map(|l| l.valid).unwrap_or(false)
    }

    /// Borrow the static-layer view, if the cache has been
    /// allocated. The compositor uses this as the render target for
    /// the static paint pass (everything goes into the offscreen
    /// texture instead of the surface).
    pub fn static_layer_view(&self) -> Option<&wgpu::TextureView> {
        self.static_layer.as_ref().map(|l| &l.view)
    }

    /// Mark the static-layer cache freshly painted. Called by the
    /// compositor after `render_static_layer` (or after an inner
    /// `render_with_clear` that targeted the static layer view)
    /// completes — flips `static_layer_valid()` to true so the next
    /// fast-path frame blits instead of repainting.
    pub fn mark_static_layer_valid(&mut self) {
        if let Some(layer) = self.static_layer.as_mut() {
            layer.valid = true;
        }
    }

    /// Render `batch` into the static-cache texture. The caller is
    /// expected to have called `ensure_static_layer` first; this
    /// method asserts in debug and no-ops in release if the cache is
    /// missing. After this returns, [`Self::static_layer_valid`]
    /// reads `true` and [`Self::blit_static_layer_into`] will copy
    /// the freshly-painted cache onto the surface.
    pub fn render_static_layer(&mut self, batch: &PrimitiveBatch, clear_color: [f64; 4]) {
        // Take the view out as a clone-of-handle so we can pass it
        // to render_with_clear without holding a borrow into
        // self.static_layer (render_with_clear needs `&mut self`).
        let view = match &self.static_layer {
            Some(layer) => layer.view.clone(),
            None => {
                debug_assert!(false, "render_static_layer without ensure_static_layer");
                return;
            }
        };
        self.render_with_clear(&view, batch, clear_color);
        if let Some(layer) = self.static_layer.as_mut() {
            layer.valid = true;
        }
    }

    /// Re-render the static layer only within `damage_rects`. Pixels
    /// outside the union are untouched; pixels inside are cleared and
    /// re-drawn from every primitive in `batch` whose AABB intersects
    /// the union. The static layer stays `valid` afterwards.
    ///
    /// Returns `false` when the cache isn't valid or non-SDF content
    /// the damage path doesn't yet support (paths, 3D viewports,
    /// particles) is present in the batch — caller falls back to a
    /// full repaint.
    ///
    /// Layer commands ARE supported (Phase 4d Opt 2): batches with
    /// push / pop pairs walk the same effect-layer logic that
    /// `render_with_layer_effects` uses, then composite each effect
    /// layer back into the static cache with `pending_damage_scissor`
    /// set so the blit lands only inside the damage union. Layers
    /// whose bounds don't intersect the damage union are skipped
    /// entirely.
    pub fn render_static_layer_damaged(
        &mut self,
        damage_rects: &[[f32; 4]],
        batch: &PrimitiveBatch,
    ) -> bool {
        let Some(layer) = self.static_layer.as_ref() else {
            return false;
        };
        if !layer.valid {
            return false;
        }
        // Bail to full repaint when the batch has anything the
        // damage path doesn't yet support. Paths / 3D viewports /
        // particles need their own scissored dispatch routines and
        // are out of scope for this commit; layer_commands are
        // handled below.
        if !batch.paths.vertices.is_empty()
            || !batch.viewports_3d.is_empty()
            || !batch.particle_viewports.is_empty()
        {
            return false;
        }
        if damage_rects.is_empty() {
            // No damage — caller can skip the dispatch entirely.
            return true;
        }

        let view = layer.view.clone();
        let layer_width = layer.width as f32;
        let layer_height = layer.height as f32;

        // Union all damage rects into a single scissor (wgpu only
        // supports one scissor per pass). Clamp to layer extent —
        // a damage rect can extend past the cache when a binding's
        // primitives moved off-screen.
        //
        // Pad outward by `AABB_PAD` pixels. SDF primitives anti-
        // alias at the edges of their `bounds`; without padding,
        // 1-pixel AA pixels at the trailing edge of the moved
        // element stay in the cache from the previous frame and
        // produce a faint "vibrating" outline as the bounds round
        // sub-pixel positions differently each frame. 4 px is well
        // over the AA distance for typical UI font sizes and
        // corner radii (~1-2 px) while still keeping the damage
        // rect proportional to the actual movement.
        const AABB_PAD: f32 = 4.0;
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for [x, y, w, h] in damage_rects.iter().copied() {
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            min_x = min_x.min(x - AABB_PAD);
            min_y = min_y.min(y - AABB_PAD);
            max_x = max_x.max(x + w + AABB_PAD);
            max_y = max_y.max(y + h + AABB_PAD);
        }
        if !min_x.is_finite() || max_x <= min_x || max_y <= min_y {
            return true;
        }
        let scissor_x = min_x.max(0.0).floor() as u32;
        let scissor_y = min_y.max(0.0).floor() as u32;
        let scissor_right = max_x.min(layer_width).ceil() as u32;
        let scissor_bottom = max_y.min(layer_height).ceil() as u32;
        if scissor_right <= scissor_x || scissor_bottom <= scissor_y {
            return true;
        }
        let scissor_w = scissor_right - scissor_x;
        let scissor_h = scissor_bottom - scissor_y;

        // Phase 4d Opt 2: build the effect-layer list from
        // `batch.layer_commands` (mirrors `render_with_layer_effects`
        // exactly, including the Phase 4a relax that lets pure-
        // opacity layers reach the offscreen-composite path). Each
        // entry's primitive range gets excluded from the SDF pass
        // below — those primitives render into their layer's tight
        // texture instead. The layer-composite blit lands inside
        // the damage scissor via `pending_damage_scissor`.
        let mut effect_layers: Vec<EffectLayerRange> = Vec::new();
        let mut effect_primitive_indices: std::collections::HashSet<usize> =
            std::collections::HashSet::new();
        if !batch.layer_commands.is_empty() {
            use crate::primitives::LayerCommand;
            let mut layer_stack: Vec<(LayerStackFrame, blinc_core::LayerConfig)> = Vec::new();
            for entry in &batch.layer_commands {
                match &entry.command {
                    LayerCommand::Push { config } => {
                        layer_stack.push((
                            LayerStackFrame {
                                primitive_start: entry.primitive_index,
                                path_index_start: entry.path_index_count,
                                path_vertex_start: entry.path_vertex_index,
                            },
                            config.clone(),
                        ));
                    }
                    LayerCommand::Pop => {
                        if let Some((start, config)) = layer_stack.pop() {
                            let has_effect_or_blend_or_3d = !config.effects.is_empty()
                                || config.blend_mode != blinc_core::BlendMode::Normal
                                || config.transform_3d.is_some();
                            let has_pure_opacity = config.opacity < 1.0;
                            if has_effect_or_blend_or_3d || has_pure_opacity {
                                effect_layers.push(EffectLayerRange {
                                    primitive_start: start.primitive_start,
                                    primitive_end: entry.primitive_index,
                                    path_index_start: start.path_index_start,
                                    path_index_end: entry.path_index_count,
                                    path_vertex_start: start.path_vertex_start,
                                    path_vertex_end: entry.path_vertex_index,
                                    config,
                                });
                            }
                        }
                    }
                    LayerCommand::Sample { .. } => {}
                }
            }
            for layer in &effect_layers {
                for i in layer.primitive_start..layer.primitive_end {
                    effect_primitive_indices.insert(i);
                }
            }
        }

        // Cull primitives to only those intersecting the union rect
        // AND not inside an effect layer. The static cache outside
        // the scissor is preserved by `LoadOp::Load`, so we skip
        // every prim whose AABB doesn't touch the damaged region.
        // Effect-layer prims render separately into their layer's
        // offscreen texture below.
        let scissor_rect = [
            scissor_x as f32,
            scissor_y as f32,
            scissor_w as f32,
            scissor_h as f32,
        ];
        let visible: Vec<GpuPrimitive> = batch
            .primitives
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                if effect_primitive_indices.contains(i) {
                    return false;
                }
                let [x, y, w, h] = p.bounds;
                if w <= 0.0 || h <= 0.0 {
                    return false;
                }
                aabb_intersects(scissor_rect, [x, y, w, h])
            })
            .map(|(_, p)| p)
            .copied()
            .collect();

        // Update uniforms (viewport is the cache extent, not the
        // window — primitives were emitted into cache pixel space).
        let uniforms = Uniforms {
            viewport_size: [layer_width, layer_height],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        let sdf_ranges = if visible.is_empty() {
            SdfPrimitiveRanges::default()
        } else {
            self.upload_sorted_primitives(&visible)
        };
        self.update_aux_data_buffer(batch);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blinc-damage-rect-encoder"),
            });

        // Single-pass damage-rect re-render. wgpu's `LoadOp::Clear`
        // ignores `set_scissor_rect`, so we can't clear-inside-
        // scissor via attachment ops. Instead we open ONE render
        // pass with `LoadOp::Load` and issue two draws inside it:
        //
        //   1. clear-quad pipeline (REPLACE blend, writes opaque
        //      black) → with the active scissor, zeros the damaged
        //      region.
        //   2. SDF pipeline → re-paints the cleared region in
        //      z-order from every primitive whose AABB intersects
        //      the union damage rect.
        //
        // Two passes was the original cut but every render-pass
        // begin/end forces a tile-memory load + store on Metal
        // (the static cache is a multi-MB texture). Folding both
        // draws into a single pass means one load + one store per
        // damage frame instead of two — measurable on cn_demo's
        // switch / progress / slider animations where the slow
        // path's single-pass full-clear was visibly smoother than
        // the two-pass damage path despite doing more total work.
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blinc-damage-rect-combined"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_w, scissor_h);
            // First draw: scissored clear via the clear-quad pipeline.
            render_pass.set_pipeline(&self.pipelines.clear_quad);
            render_pass.draw(0..3, 0..1);
            // Second draw: SDF re-paint inside the just-cleared scissor.
            if !visible.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    false,
                    self.sdf_vb_buffer(),
                );
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Phase 4d Opt 2: composite each effect layer back into the
        // static cache with `pending_damage_scissor` set so the blit
        // lands only inside the damage union. Layers whose bounds
        // don't intersect the damage rect are skipped entirely —
        // their previous-frame pixels stay in the cache (preserved
        // by `LoadOp::Load` in the SDF pass above).
        //
        // The blit's own visible-bounds scissor intersects with
        // `pending_damage_scissor` inside `blit_tight_texture_to_target`
        // (commit e0286ef0). Empty intersection ⇒ the blit's render
        // pass submits as a no-op.
        if !effect_layers.is_empty() {
            let damage_scissor = (scissor_x, scissor_y, scissor_w, scissor_h);
            let damage_rect_f = [
                scissor_x as f32,
                scissor_y as f32,
                (scissor_x + scissor_w) as f32,
                (scissor_y + scissor_h) as f32,
            ];
            for layer in effect_layers {
                let primitives = &batch.primitives[layer.primitive_start..layer.primitive_end];
                let path_verts: &[PathVertex] = if layer.path_vertex_end > layer.path_vertex_start {
                    &batch.paths.vertices[layer.path_vertex_start..layer.path_vertex_end]
                } else {
                    &[]
                };
                if primitives.is_empty() && path_verts.is_empty() {
                    continue;
                }

                // Compute layer's screen-space bounding box (same
                // logic as `render_with_layer_effects`). Path vertex
                // positions matter for Lottie-style content; SDF
                // primitives carry their bbox in `bounds`.
                let mut min_x = f32::MAX;
                let mut min_y = f32::MAX;
                let mut max_x = f32::MIN;
                let mut max_y = f32::MIN;
                let mut clip: Option<([f32; 4], [f32; 4])> = None;
                for p in primitives {
                    let (px, py, pw, ph) = (p.bounds[0], p.bounds[1], p.bounds[2], p.bounds[3]);
                    min_x = min_x.min(px);
                    min_y = min_y.min(py);
                    max_x = max_x.max(px + pw);
                    max_y = max_y.max(py + ph);
                    if clip.is_none() && p.clip_bounds[0] > -5000.0 && p.clip_bounds[2] < 90000.0 {
                        clip = Some((p.clip_bounds, p.clip_radius));
                    }
                }
                for v in path_verts {
                    min_x = min_x.min(v.position[0]);
                    min_y = min_y.min(v.position[1]);
                    max_x = max_x.max(v.position[0]);
                    max_y = max_y.max(v.position[1]);
                    if clip.is_none() && v.clip_bounds[0] > -5000.0 && v.clip_bounds[2] < 90000.0 {
                        clip = Some((v.clip_bounds, v.clip_radius));
                    }
                }
                let layer_width = (max_x - min_x).max(1.0);
                let layer_height = (max_y - min_y).max(1.0);
                let layer_pos = (min_x, min_y);
                let layer_size = (layer_width, layer_height);
                let layer_clip = clip;

                // Layer ∩ damage check — skip if disjoint. Compare
                // pre-effect-expansion bounds (post-expansion bounds
                // get computed below for the blit destination, but
                // the *content* lives inside the raw bounds and an
                // empty raw ∩ damage means no work needed).
                if layer_pos.0 + layer_size.0 <= damage_rect_f[0]
                    || layer_pos.1 + layer_size.1 <= damage_rect_f[1]
                    || layer_pos.0 >= damage_rect_f[2]
                    || layer_pos.1 >= damage_rect_f[3]
                {
                    continue;
                }

                // Skip if entirely outside viewport (same as
                // `render_with_layer_effects`).
                let vp_w = self.viewport_size.0 as f32;
                let vp_h = self.viewport_size.1 as f32;
                let is_visible = layer_pos.0 < vp_w
                    && layer_pos.1 < vp_h
                    && layer_pos.0 + layer_size.0 > 0.0
                    && layer_pos.1 + layer_size.1 > 0.0
                    && layer_size.0 > 0.0
                    && layer_size.1 > 0.0;
                if !is_visible {
                    continue;
                }

                let config = layer.config.clone();
                let effect_expansion = Self::calculate_effect_expansion(&config.effects);

                // Render layer to tight offscreen texture (helper
                // already exists; same call shape as
                // `render_with_layer_effects`).
                let (layer_texture, content_size) = self.render_layer_range_tight(
                    batch,
                    layer.primitive_start,
                    layer.primitive_end,
                    layer.path_vertex_start,
                    layer.path_vertex_end,
                    layer.path_index_start,
                    layer.path_index_end,
                    layer_pos,
                    layer_size,
                    effect_expansion,
                );
                let tight_size = content_size;
                let expanded_pos = (
                    layer_pos.0 - effect_expansion.0,
                    layer_pos.1 - effect_expansion.1,
                );
                let expanded_size = (
                    layer_size.0 + effect_expansion.0 + effect_expansion.2,
                    layer_size.1 + effect_expansion.1 + effect_expansion.3,
                );

                // The damage scissor is the intersection rule:
                // `blit_tight_texture_to_target` intersects its own
                // visible-bounds scissor with this, so the blit
                // paints only inside (layer content area) ∩ (damage
                // rect). Empty intersection ⇒ the blit is a no-op.
                self.set_pending_damage_scissor(damage_scissor);

                if config.effects.is_empty() {
                    self.blit_tight_texture_to_target(
                        &layer_texture.view,
                        tight_size,
                        &view,
                        expanded_pos,
                        expanded_size,
                        config.opacity,
                        config.blend_mode,
                        layer_clip,
                        config.transform_3d,
                    );
                    self.layer_texture_cache.release(layer_texture);
                } else {
                    let effected = self.apply_layer_effects(&layer_texture, &config.effects);
                    self.layer_texture_cache.release(layer_texture);
                    self.blit_tight_texture_to_target(
                        &effected.view,
                        tight_size,
                        &view,
                        expanded_pos,
                        expanded_size,
                        config.opacity,
                        config.blend_mode,
                        layer_clip,
                        config.transform_3d,
                    );
                    self.layer_texture_cache.release(effected);
                }

                self.clear_pending_damage_scissor();
            }
        }

        true
    }

    /// `copy_texture_to_texture` the static-cache onto the supplied
    /// surface texture. Caller must use the underlying `wgpu::Texture`
    /// (not a `TextureView`) because that's what `CommandEncoder::
    /// copy_texture_to_texture` requires. Returns `false` if the
    /// cache hasn't been painted yet — in that case the caller
    /// should fall back to a full repaint.
    pub fn blit_static_layer_into(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target_texture: &wgpu::Texture,
    ) -> bool {
        let Some(layer) = &self.static_layer else {
            return false;
        };
        if !layer.valid {
            return false;
        }
        let extent = wgpu::Extent3d {
            width: layer.width,
            height: layer.height,
            depth_or_array_layers: 1,
        };
        encoder.copy_texture_to_texture(
            layer.texture.as_image_copy(),
            target_texture.as_image_copy(),
            extent,
        );
        true
    }

    /// Layer-compositor composite step: blit the cached static
    /// layer onto the surface and then dispatch `overlay_primitives`
    /// (typically canvas content re-emitted this frame) on top.
    ///
    /// Done in a single command-encoder submission so the GPU sees
    /// blit-then-load-then-draw as one ordered batch — no extra
    /// submit overhead vs the previous full-paint path.
    ///
    /// Returns `false` if the static layer hasn't been painted yet;
    /// the caller should fall back to a full repaint into the cache
    /// before retrying.
    pub fn composite_frame(
        &mut self,
        target_view: &wgpu::TextureView,
        target_texture: &wgpu::Texture,
        overlay_primitives: &[GpuPrimitive],
        overlay_aux_data: &[[f32; 4]],
    ) -> bool {
        // First, validate the cache and capture the extent. We can't
        // hold a borrow into `self.static_layer` across the upload
        // calls below (they take `&mut self`), so we just record what
        // we need.
        let extent = match &self.static_layer {
            Some(layer) if layer.valid => wgpu::Extent3d {
                width: layer.width,
                height: layer.height,
                depth_or_array_layers: 1,
            },
            _ => return false,
        };

        // Upload + sort overlay primitives (smaller than the full
        // batch — typically <200 prims for cn_demo's spinners). The
        // SDF GPU buffer is shared with the static pass that wrote
        // earlier this frame, but the GPU has already consumed
        // those reads in the static-layer render pass, so the queue
        // is allowed to overwrite the buffer here.
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Upload the overlay's aux_data so polygon-clip / 3D-group
        // offsets carried on overlay primitives index into valid GPU
        // data. The static layer's aux_data was written earlier this
        // frame but has already been consumed into the cache texture;
        // overwriting the buffer here is safe and ensures the overlay
        // SDF dispatch reads the right vertex / shape descriptors.
        self.update_aux_data_slice(overlay_aux_data);

        let visible_overlay: Vec<GpuPrimitive> = overlay_primitives
            .iter()
            .filter(|p| self.is_primitive_visible(p))
            .copied()
            .collect();
        let sdf_ranges = if visible_overlay.is_empty() {
            SdfPrimitiveRanges::default()
        } else {
            self.upload_sorted_primitives(&visible_overlay)
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blinc-compositor-encoder"),
            });

        // 1. Blit static cache onto surface (replaces all pixels).
        let source_copy = self.static_layer.as_ref().unwrap().texture.as_image_copy();
        let dest_copy = target_texture.as_image_copy();
        encoder.copy_texture_to_texture(source_copy, dest_copy, extent);

        // 2. Draw overlay primitives on top with `LoadOp::Load` so
        //    the just-blitted cache content is preserved.
        if !visible_overlay.is_empty() {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blinc-compositor-overlay-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
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
            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            Self::draw_split_sdf(
                &mut render_pass,
                &self.pipelines,
                &sdf_ranges,
                false,
                self.sdf_vb_buffer(),
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        true
    }

    ///
    /// # Arguments
    /// * `target` - The texture view to render to
    /// * `batch` - The primitive batch to render
    /// * `clear_color` - RGBA clear color (0.0-1.0 range)
    pub fn render_with_clear(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
    ) {
        // Evict oversized textures from the pool at frame start
        // This prevents memory bloat from accumulated large textures
        self.layer_texture_cache.evict_oversized();

        // Check whether any layer command needs the layer-aware path.
        // Phase 4a: include pure-opacity layers so `config.opacity`
        // actually reaches the composite blit. Without this, opacity-
        // only `@keyframes` rendered at full alpha (the simple path
        // ignores layer commands entirely).
        let has_layer_effects = batch.layer_commands.iter().any(|entry| {
            if let crate::primitives::LayerCommand::Push { config } = &entry.command {
                !config.effects.is_empty()
                    || config.blend_mode != blinc_core::BlendMode::Normal
                    || config.transform_3d.is_some()
                    || config.opacity < 1.0
            } else {
                false
            }
        });

        tracing::trace!(
            "render_with_clear: {} primitives, {} layer commands, has_layer_effects={}",
            batch.primitives.len(),
            batch.layer_commands.len(),
            has_layer_effects
        );

        // If we have layer effects, use the layer-aware rendering path
        if has_layer_effects {
            self.render_with_layer_effects(target, batch, clear_color);
            return;
        }

        // Standard rendering (no layer effects)
        self.render_with_clear_simple(target, batch, clear_color);
    }

    /// Simple render with clear (no layer effect processing)
    /// Test whether a primitive's expanded bounds intersect the viewport.
    ///
    /// Returns `true` if the primitive might be visible and should be rendered.
    /// Conservative: accounts for shadow, border, and rotation expansion.
    /// Primitives with 3D perspective are always considered visible.
    #[inline]
    fn is_primitive_visible(&self, prim: &GpuPrimitive) -> bool {
        let vp_w = self.viewport_size.0 as f32;
        let vp_h = self.viewport_size.1 as f32;

        let px = prim.bounds[0];
        let py = prim.bounds[1];
        let pw = prim.bounds[2];
        let ph = prim.bounds[3];

        // Primitives with 3D perspective may project anywhere
        if prim.perspective[2] > 0.0 {
            return true;
        }

        // Text glyph primitives (prim_type 7) bypass the
        // viewport AABB cull. A scroll / layer container translates
        // its children at composite time, so glyph bounds live in
        // content (pre-translate) space — a glyph at content y=2000
        // that's visible on screen after a -1000 scroll would otherwise
        // get dropped here. The fragment shader's clip / alpha already
        // discards fragments outside the layer bounds, so skipping this
        // cull is cheap: the primitive either clips to nothing (free)
        // or renders at the correct place.
        if prim.type_info[0] == 7 {
            return true;
        }

        // Account for shadow expansion (matches shader bounds computation)
        let shadow_blur = prim.shadow[2];
        let shadow_ox = prim.shadow[0].abs();
        let shadow_oy = prim.shadow[1].abs();
        let mut expand = shadow_blur * 3.0 + shadow_ox + shadow_oy;

        // Account for border (stroke) expansion
        expand += prim.border[0];

        // Account for rotation — rotated rects have larger AABB
        // rotation = [sin_rz, cos_rz, sin_ry, cos_ry], identity = [0, 1, 0, 1]
        let has_rotation = prim.rotation[0] != 0.0 || prim.rotation[2] != 0.0;
        if has_rotation {
            // Worst case AABB expansion for rotated rect: half-diagonal
            let half_diag = (pw * pw + ph * ph).sqrt() * 0.5;
            expand += half_diag;
        }

        // Non-identity local affine — be generous with expansion
        let has_affine = prim.local_affine[1] != 0.0 || prim.local_affine[2] != 0.0;
        if has_affine {
            let half_diag = (pw * pw + ph * ph).sqrt() * 0.5;
            expand += half_diag;
        }

        // AABB intersection with viewport [0, 0, vp_w, vp_h]
        let left = px - expand;
        let top = py - expand;
        let right = px + pw + expand;
        let bottom = py + ph + expand;

        right > 0.0 && bottom > 0.0 && left < vp_w && top < vp_h
    }

    /// Cull a slice of primitives, returning only those visible in the viewport.
    fn cull_primitives(&self, prims: &[GpuPrimitive]) -> Vec<GpuPrimitive> {
        prims
            .iter()
            .filter(|p| self.is_primitive_visible(p))
            .copied()
            .collect()
    }

    fn render_with_clear_simple(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
    ) {
        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Cull off-screen primitives before GPU upload
        let visible_primitives = self.cull_primitives(&batch.primitives);

        // Sort primitives by pipeline category and upload
        let sdf_ranges = self.upload_sorted_primitives(&visible_primitives);

        // Update auxiliary data buffer (group shapes, polygon clips)
        // This may call rebind_sdf_bind_group() if the buffer needs resizing.
        // When active_glyph_atlas is set, rebind uses the real atlas automatically.
        self.update_aux_data_buffer(batch);

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Render Encoder"),
            });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0],
                            g: clear_color[1],
                            b: clear_color[2],
                            a: clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render SDF primitives via split pipelines.
            if !visible_primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    false,
                    self.sdf_vb_buffer(),
                );
            }

            // Render tessellated paths in a single monolithic draw. This
            // places every path on top of every SDF primitive regardless
            // of submission order, which is the right default for most
            // Blinc UI (SVG icons over background cards, canvas fills
            // over SDF text, etc). For Lottie scenes whose layer stack
            // interleaves text (SDF primitive stream via `draw_text`)
            // with shape fills (tessellated path pipeline), this order
            // can bury text under later shape fills even though Lottie's
            // back-to-front order would put the text on top. The
            // follow-up is to route path fills through the SDF pipeline
            // as a new primitive type so the existing submission-order
            // dispatch handles them the same way it handles text — see
            // BACKLOG.md for the plan.
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(&self.pipelines.path);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
                }
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));

        // Render SDF 3D viewports (after main content, so they render on top)
        if !batch.viewports_3d.is_empty() {
            self.render_sdf_3d_viewports(target, &batch.viewports_3d);
        }

        // Render GPU particle viewports (after SDF viewports)
        if !batch.particle_viewports.is_empty() {
            self.render_particle_viewports(target, &batch.particle_viewports);
        }
    }

    /// Render with layer effect processing
    ///
    /// This implements a correct layer effect system:
    /// 1. Identify primitive ranges for effect layers
    /// 2. Render non-effect primitives to target (skipping those in effect layers)
    /// 3. For each effect layer, render to viewport-sized texture, apply effects, blit at position
    fn render_with_layer_effects(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
    ) {
        use crate::primitives::LayerCommand;

        // Build list of effect layers with their primitive AND path
        // ranges. Tracking path ranges (not just primitive indices)
        // is what lets translucent-opacity Lottie layers — whose
        // shapes live entirely in `batch.paths` — reach the
        // offscreen composite instead of bypassing it.
        let mut effect_layers: Vec<EffectLayerRange> = Vec::new();
        let mut layer_stack: Vec<(LayerStackFrame, blinc_core::LayerConfig)> = Vec::new();

        for entry in &batch.layer_commands {
            match &entry.command {
                LayerCommand::Push { config } => {
                    layer_stack.push((
                        LayerStackFrame {
                            primitive_start: entry.primitive_index,
                            path_index_start: entry.path_index_count,
                            path_vertex_start: entry.path_vertex_index,
                        },
                        config.clone(),
                    ));
                }
                LayerCommand::Pop => {
                    if let Some((start, config)) = layer_stack.pop() {
                        // Phase 4a: include pure-opacity layers
                        // (`config.opacity < 1.0` alone). Pre-fix the
                        // gate required effects / blend / 3D; pure-
                        // opacity layers were silently dropped — their
                        // primitives rendered at full alpha in the
                        // first pass and `config.opacity` went
                        // nowhere. The walker's hybrid flatten now
                        // skips `push_layer` for simple-enough opacity
                        // subtrees (children.len() <= 1) so those
                        // don't reach this gate at all; what does
                        // reach is the cases that genuinely need a
                        // layer composite (overlapping children,
                        // nested opacity), and they get correct
                        // offscreen-then-composite rendering via
                        // `blit_tight_texture_to_target`, which reads
                        // `config.opacity`.
                        let has_effect_or_blend_or_3d = !config.effects.is_empty()
                            || config.blend_mode != blinc_core::BlendMode::Normal
                            || config.transform_3d.is_some();
                        let has_pure_opacity = config.opacity < 1.0;
                        if has_effect_or_blend_or_3d || has_pure_opacity {
                            effect_layers.push(EffectLayerRange {
                                primitive_start: start.primitive_start,
                                primitive_end: entry.primitive_index,
                                path_index_start: start.path_index_start,
                                path_index_end: entry.path_index_count,
                                path_vertex_start: start.path_vertex_start,
                                path_vertex_end: entry.path_vertex_index,
                                config,
                            });
                        }
                    }
                }
                LayerCommand::Sample { .. } => {}
            }
        }

        // If no effect layers, just render normally
        if effect_layers.is_empty() {
            self.render_with_clear_simple(target, batch, clear_color);
            return;
        }

        // Build set of primitive indices AND path-index ranges that
        // belong to effect layers so the first pass knows what to
        // skip. Paths are skipped as whole `[start..end)` ranges —
        // the draw call is split around them rather than using a
        // per-index hash set (index buffers are much larger than
        // the primitive count).
        let mut effect_primitives = std::collections::HashSet::new();
        let mut path_skip_ranges: Vec<(u32, u32)> = Vec::new();
        for layer in &effect_layers {
            for i in layer.primitive_start..layer.primitive_end {
                effect_primitives.insert(i);
            }
            if layer.path_index_end > layer.path_index_start {
                path_skip_ranges.push((layer.path_index_start as u32, layer.path_index_end as u32));
            }
        }
        path_skip_ranges.sort_unstable();

        // First pass: render primitives + paths that are NOT in effect layers
        self.render_primitives_excluding_with_paths(
            target,
            batch,
            &effect_primitives,
            &path_skip_ranges,
            clear_color,
        );
        drop(effect_primitives); // Free HashSet immediately - not needed after first pass

        // Process each effect layer
        for layer in effect_layers {
            let EffectLayerRange {
                primitive_start: start_idx,
                primitive_end: end_idx,
                config,
                ..
            } = &layer;
            let (start_idx, end_idx) = (*start_idx, *end_idx);
            let config = config.clone();
            let has_primitives = start_idx < end_idx && end_idx <= batch.primitives.len();
            let has_paths = layer.path_index_end > layer.path_index_start
                && layer.path_index_end <= batch.paths.indices.len();
            if !has_primitives && !has_paths {
                continue;
            }

            // Compute the layer's screen-space bounding box from its
            // primitives + path vertices. Lottie layers contain only
            // paths (tessellated by `fill_path`), no SDF primitives —
            // so the bbox MUST include path vertices, otherwise the
            // tight texture would be sized from the LayerConfig
            // fallback (often viewport-sized) and the offscreen blit
            // would be in the wrong position.
            let primitives = &batch.primitives[start_idx..end_idx];
            let path_verts = if has_paths {
                &batch.paths.vertices[layer.path_vertex_start..layer.path_vertex_end]
            } else {
                &[][..]
            };
            let (layer_pos, layer_size, layer_clip) = if primitives.is_empty()
                && path_verts.is_empty()
            {
                let pos = config.position.map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
                let size = config
                    .size
                    .map(|s| (s.width, s.height))
                    .unwrap_or((self.viewport_size.0 as f32, self.viewport_size.1 as f32));
                (pos, size, None)
            } else {
                let mut min_x = f32::MAX;
                let mut min_y = f32::MAX;
                let mut max_x = f32::MIN;
                let mut max_y = f32::MIN;
                let mut clip: Option<([f32; 4], [f32; 4])> = None;
                for p in primitives {
                    let (px, py, pw, ph) = (p.bounds[0], p.bounds[1], p.bounds[2], p.bounds[3]);
                    min_x = min_x.min(px);
                    min_y = min_y.min(py);
                    max_x = max_x.max(px + pw);
                    max_y = max_y.max(py + ph);
                    if clip.is_none() && p.clip_bounds[0] > -5000.0 && p.clip_bounds[2] < 90000.0 {
                        clip = Some((p.clip_bounds, p.clip_radius));
                    }
                }
                for v in path_verts {
                    min_x = min_x.min(v.position[0]);
                    min_y = min_y.min(v.position[1]);
                    max_x = max_x.max(v.position[0]);
                    max_y = max_y.max(v.position[1]);
                    if clip.is_none() && v.clip_bounds[0] > -5000.0 && v.clip_bounds[2] < 90000.0 {
                        clip = Some((v.clip_bounds, v.clip_radius));
                    }
                }
                let width = (max_x - min_x).max(1.0);
                let height = (max_y - min_y).max(1.0);
                ((min_x, min_y), (width, height), clip)
            };

            // Skip layers that are entirely outside the viewport
            let vp_w = self.viewport_size.0 as f32;
            let vp_h = self.viewport_size.1 as f32;
            let is_visible = layer_pos.0 < vp_w
                && layer_pos.1 < vp_h
                && layer_pos.0 + layer_size.0 > 0.0
                && layer_pos.1 + layer_size.1 > 0.0
                && layer_size.0 > 0.0
                && layer_size.1 > 0.0;

            if !is_visible {
                continue;
            }

            // Calculate effect expansion (how much effects extend beyond original bounds)
            let effect_expansion = Self::calculate_effect_expansion(&config.effects);

            // Render layer primitives + paths to a TIGHT texture
            // (not viewport-sized!). Path range comes from the
            // recorded `LayerCommandEntry`, so Lottie shapes —
            // which only ever produce path geometry — get their
            // own slice composed at full alpha into the offscreen
            // before the blit applies the layer's opacity.
            let (layer_texture, content_size) = self.render_layer_range_tight(
                batch,
                start_idx,
                end_idx,
                layer.path_vertex_start,
                layer.path_vertex_end,
                layer.path_index_start,
                layer.path_index_end,
                layer_pos,
                layer_size,
                effect_expansion,
            );

            // Use content_size for blitting (not layer_texture.size which may be larger)
            let tight_size = content_size;

            // Calculate the destination position and size for blitting
            // Don't clamp to 0 - allow negative positions for scrolled content
            // The blit function will handle off-screen portions correctly
            let expanded_pos = (
                layer_pos.0 - effect_expansion.0,
                layer_pos.1 - effect_expansion.1,
            );
            let expanded_size = (
                layer_size.0 + effect_expansion.0 + effect_expansion.2,
                layer_size.1 + effect_expansion.1 + effect_expansion.3,
            );

            // Skip texture copy when no effects - use layer_texture directly
            if config.effects.is_empty() {
                // Blit directly without effect processing (skip copy)
                self.blit_tight_texture_to_target(
                    &layer_texture.view,
                    tight_size,
                    target,
                    expanded_pos,
                    expanded_size,
                    config.opacity,
                    config.blend_mode,
                    layer_clip,
                    config.transform_3d,
                );
                self.layer_texture_cache.release(layer_texture);
            } else {
                // Apply effects to the tight texture
                let effected = self.apply_layer_effects(&layer_texture, &config.effects);
                self.layer_texture_cache.release(layer_texture);

                // Blit the effected texture back to target at the correct position
                // Pass through the clip bounds so effects don't bleed outside scroll containers
                self.blit_tight_texture_to_target(
                    &effected.view,
                    tight_size,
                    target,
                    expanded_pos,
                    expanded_size,
                    config.opacity,
                    config.blend_mode,
                    layer_clip,
                    config.transform_3d,
                );
                self.layer_texture_cache.release(effected);
            }
        }
    }

    /// Render primitives excluding those in the given set
    fn render_primitives_excluding(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        exclude: &std::collections::HashSet<usize>,
        clear_color: [f64; 4],
    ) {
        self.render_primitives_excluding_with_paths(target, batch, exclude, &[], clear_color)
    }

    /// Like [`Self::render_primitives_excluding`] but also skips a
    /// set of path-index ranges from the single path draw call. The
    /// ranges must be sorted by start index and non-overlapping —
    /// callers get that from `EffectLayerRange` values built by
    /// `render_with_layer_effects`, which emits layers in a
    /// stack-ordered, non-overlapping manner.
    fn render_primitives_excluding_with_paths(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        exclude: &std::collections::HashSet<usize>,
        path_skip_ranges: &[(u32, u32)],
        clear_color: [f64; 4],
    ) {
        // If nothing to exclude, use simple path
        if exclude.is_empty() && path_skip_ranges.is_empty() {
            self.render_with_clear_simple(target, batch, clear_color);
            return;
        }

        // Build list of primitives to render (excluding effect layers + off-screen)
        let included_primitives: Vec<GpuPrimitive> = batch
            .primitives
            .iter()
            .enumerate()
            .filter(|(i, p)| !exclude.contains(i) && self.is_primitive_visible(p))
            .map(|(_, p)| *p)
            .collect();

        if included_primitives.is_empty() && batch.paths.vertices.is_empty() {
            // Just clear the target
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Clear Encoder"),
                });
            {
                let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: clear_color[0],
                                g: clear_color[1],
                                b: clear_color[2],
                                a: clear_color[3],
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            return;
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update auxiliary data buffer
        self.update_aux_data_buffer(batch);

        // Sort and upload filtered primitives
        let sdf_ranges = self.upload_sorted_primitives(&included_primitives);

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Filtered Render Encoder"),
            });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Filtered Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0],
                            g: clear_color[1],
                            b: clear_color[2],
                            a: clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render SDF primitives via split pipelines (filtered)
            if !included_primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    false,
                    self.sdf_vb_buffer(),
                );
            }

            // Render paths, skipping any index ranges that belong
            // to translucent / effect layers (those are composited
            // offscreen in the second pass). Paths in non-effect
            // ranges still render here directly to the target.
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(&self.pipelines.path);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    let total = batch.paths.indices.len() as u32;
                    let mut cursor = 0u32;
                    for &(skip_start, skip_end) in path_skip_ranges {
                        if skip_start > cursor {
                            render_pass.draw_indexed(cursor..skip_start, 0, 0..1);
                        }
                        cursor = cursor.max(skip_end);
                    }
                    if cursor < total {
                        render_pass.draw_indexed(cursor..total, 0, 0..1);
                    }
                }
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Update auxiliary data buffer (for 3D group shapes, polygon clips, etc.)
    ///
    /// If the batch has aux_data, writes it to the GPU buffer, recreating the buffer
    /// and rebinding if it's too small.
    fn update_aux_data_buffer(&mut self, batch: &PrimitiveBatch) {
        self.update_aux_data_slice(&batch.aux_data);
    }

    /// Public wrapper for callers in `blinc_app` that need to
    /// re-upload a batch's aux_data after another batch's render
    /// pass clobbered the shared GPU buffer. Used by the
    /// non-compositor path's dynamic-batch dispatch: after the
    /// motion-bound batch's aux upload + draw, the static batch's
    /// polygon clip / 3D-group descriptor offsets would otherwise
    /// reference stale data on downstream dispatches.
    pub fn update_aux_data_for_batch(&mut self, batch: &PrimitiveBatch) {
        self.update_aux_data_buffer(batch);
    }

    /// Slice-variant of [`Self::update_aux_data_buffer`] for callers that
    /// only have an `&[[f32; 4]]` (e.g. the compositor overlay path
    /// which carries the dynamic batch's aux_data separately from a
    /// `PrimitiveBatch`). Avoids constructing a throwaway batch just
    /// to satisfy the buffer-variant signature.
    fn update_aux_data_slice(&mut self, aux_data: &[[f32; 4]]) {
        if aux_data.is_empty() {
            return;
        }

        if !self.has_storage_buffers {
            // DT mode: upload aux data to texture instead of storage buffer
            self.update_aux_data_texture(aux_data);
            return;
        }

        let data_size = std::mem::size_of_val(aux_data) as u64;
        let buffer_size = self.buffers.aux_data.size();

        // Recreate buffer if too small
        if data_size > buffer_size {
            self.buffers.aux_data = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Aux Data Buffer"),
                size: data_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // Must recreate the SDF and path bind groups since the
            // buffer changed — path binding(7) references aux_data too
            // so the same handle-invalidation applies.
            self.rebind_sdf_bind_group();
            self.rebind_path_bind_group();
        }

        self.queue
            .write_buffer(&self.buffers.aux_data, 0, bytemuck::cast_slice(aux_data));
    }

    /// Upload auxiliary data to the DT fallback texture (Tier 3 / WebGL2).
    ///
    /// The texture has width=1024 and variable height. If the data exceeds
    /// the current texture capacity, the texture is recreated larger and
    /// the SDF bind group is rebound.
    fn update_aux_data_texture(&mut self, aux_data: &[[f32; 4]]) {
        const AUX_TEX_WIDTH: u32 = 1024;
        let count = aux_data.len() as u32;
        let needed_height = count.div_ceil(AUX_TEX_WIDTH).max(1);

        if needed_height > self.buffers.aux_data_texture_height {
            // Recreate the texture with more rows
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Aux Data Texture"),
                size: wgpu::Extent3d {
                    width: AUX_TEX_WIDTH,
                    height: needed_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.buffers.aux_data_texture = Some(tex);
            self.buffers.aux_data_view = Some(view);
            self.buffers.aux_data_texture_height = needed_height;

            // Rebind since the texture changed
            self.rebind_sdf_bind_group();
        }

        if let Some(ref tex) = self.buffers.aux_data_texture {
            // Pad aux_data to full rows so write_texture gets a complete rectangle
            let total_texels = (AUX_TEX_WIDTH * needed_height) as usize;
            let mut padded = aux_data.to_vec();
            padded.resize(total_texels, [0.0f32; 4]);

            let bytes = bytemuck::cast_slice::<[f32; 4], u8>(&padded);
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(AUX_TEX_WIDTH * 16), // 1024 texels × 16 bytes
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: AUX_TEX_WIDTH,
                    height: needed_height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// Recreate the path bind group. Needed when the aux_data storage
    /// buffer is resized (binding 7 references the buffer by handle,
    /// so the old bind group keeps pointing at the freed buffer). Uses
    /// the placeholder path-image view — glass backdrops are applied
    /// through a separate bind group set that carries the real
    /// backdrop texture, so binding 5 here stays a placeholder.
    fn rebind_path_bind_group(&mut self) {
        self.bind_groups.path = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Path Bind Group (rebound)"),
            layout: &self.bind_group_layouts.path,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.buffers.path_uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.gradient_texture_cache.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.gradient_texture_cache.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.placeholder_path_image_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&self.placeholder_path_image_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: self.buffers.aux_data.as_entire_binding(),
                },
            ],
        });
    }

    /// Recreate the SDF bind group (needed when aux_data buffer is resized).
    ///
    /// Uses the real glyph atlas if `active_glyph_atlas` is set, otherwise
    /// falls back to placeholder textures.
    fn rebind_sdf_bind_group(&mut self) {
        // SAFETY: When active_glyph_atlas is Some, the pointers are valid for the
        // duration of the frame (they point to TextureViews owned by the text context).
        let (atlas_view, color_atlas_view): (&wgpu::TextureView, &wgpu::TextureView) =
            if let Some(active) = &self.active_glyph_atlas {
                unsafe { (&*active.atlas_view_ptr, &*active.color_atlas_view_ptr) }
            } else {
                (
                    &self.placeholder_glyph_atlas_view,
                    &self.placeholder_color_glyph_atlas_view,
                )
            };

        // Binding 1: primitives (storage buffer or data texture)
        let binding_1 = if self.has_storage_buffers {
            wgpu::BindGroupEntry {
                binding: 1,
                resource: self.buffers.primitives.as_entire_binding(),
            }
        } else {
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    self.buffers
                        .prim_data_view
                        .as_ref()
                        .expect("DT mode requires prim_data_view"),
                ),
            }
        };

        // Binding 5: aux data (storage buffer or data texture)
        let binding_5 = if self.has_storage_buffers {
            wgpu::BindGroupEntry {
                binding: 5,
                resource: self.buffers.aux_data.as_entire_binding(),
            }
        } else {
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(
                    self.buffers
                        .aux_data_view
                        .as_ref()
                        .expect("DT mode requires aux_data_view"),
                ),
            }
        };

        self.bind_groups.sdf = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF Bind Group (rebound)"),
            layout: &self.bind_group_layouts.sdf,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.buffers.uniforms.as_entire_binding(),
                },
                binding_1,
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.glyph_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(color_atlas_view),
                },
                binding_5,
            ],
        });
    }

    /// Update path vertex and index buffers
    fn update_path_buffers(&mut self, batch: &PrimitiveBatch) {
        // Upload gradient texture if needed for multi-stop gradients
        if batch.paths.use_gradient_texture {
            if let Some(ref stops) = batch.paths.gradient_stops {
                self.gradient_texture_cache.upload_stops(
                    &self.queue,
                    stops,
                    crate::gradient_texture::SpreadMode::Pad,
                );
            }
        }

        // Update path uniforms with clip data and brush metadata from batch
        let path_uniforms = PathUniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            clip_bounds: batch.paths.clip_bounds,
            clip_radius: batch.paths.clip_radius,
            clip_type: batch.paths.clip_type,
            use_gradient_texture: if batch.paths.use_gradient_texture {
                1
            } else {
                0
            },
            use_image_texture: if batch.paths.use_image_texture { 1 } else { 0 },
            use_glass_effect: if batch.paths.use_glass_effect { 1 } else { 0 },
            image_uv_bounds: batch.paths.image_uv_bounds,
            glass_params: batch.paths.glass_params,
            glass_tint: batch.paths.glass_tint,
            ..PathUniforms::default()
        };
        self.queue.write_buffer(
            &self.buffers.path_uniforms,
            0,
            bytemuck::bytes_of(&path_uniforms),
        );

        // Create or recreate vertex buffer if needed
        let vertex_size = (std::mem::size_of::<PathVertex>() * batch.paths.vertices.len()) as u64;
        let need_new_vertex_buffer = match &self.buffers.path_vertices {
            Some(buf) => buf.size() < vertex_size,
            None => true,
        };

        if need_new_vertex_buffer && vertex_size > 0 {
            self.buffers.path_vertices = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Path Vertex Buffer"),
                size: vertex_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        if let Some(vb) = &self.buffers.path_vertices {
            self.queue
                .write_buffer(vb, 0, bytemuck::cast_slice(&batch.paths.vertices));
        }

        // Create or recreate index buffer if needed
        let index_size = (std::mem::size_of::<u32>() * batch.paths.indices.len()) as u64;
        let need_new_index_buffer = match &self.buffers.path_indices {
            Some(buf) => buf.size() < index_size,
            None => true,
        };

        if need_new_index_buffer && index_size > 0 {
            self.buffers.path_indices = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Path Index Buffer"),
                size: index_size,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        if let Some(ib) = &self.buffers.path_indices {
            self.queue
                .write_buffer(ib, 0, bytemuck::cast_slice(&batch.paths.indices));
        }
    }

    /// Render primitives with MSAA (multi-sample anti-aliasing)
    ///
    /// # Arguments
    /// * `msaa_target` - The multisampled texture view to render to
    /// * `resolve_target` - The single-sampled texture view to resolve to
    /// * `batch` - The primitive batch to render
    /// * `clear_color` - RGBA clear color (0.0-1.0 range)
    pub fn render_msaa(
        &mut self,
        msaa_target: &wgpu::TextureView,
        resolve_target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
    ) {
        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(&batch.primitives);

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc MSAA Render Encoder"),
            });

        // Begin render pass with MSAA resolve
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc MSAA Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: msaa_target,
                    resolve_target: Some(resolve_target),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0],
                            g: clear_color[1],
                            b: clear_color[2],
                            a: clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render SDF primitives via split pipelines
            if !batch.primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    false,
                    self.sdf_vb_buffer(),
                );
            }

            // Render paths
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(&self.pipelines.path);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
                }
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render glass primitives (requires backdrop texture)
    ///
    /// Splits primitives into simple (frosted) and liquid (refracted) glass,
    /// rendering each with the appropriate shader.
    pub fn render_glass(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        batch: &PrimitiveBatch,
    ) {
        if batch.glass_primitives.is_empty() || !self.has_storage_buffers {
            return;
        }
        self.ensure_glass_pipelines();

        // Split primitives: simple glass first, then liquid glass
        // This allows us to render each group with its respective pipeline
        let mut simple_primitives: Vec<GpuGlassPrimitive> = Vec::new();
        let mut liquid_primitives: Vec<GpuGlassPrimitive> = Vec::new();

        for prim in &batch.glass_primitives {
            if prim.type_info[0] == GlassType::Simple as u32 {
                simple_primitives.push(*prim);
            } else {
                liquid_primitives.push(*prim);
            }
        }

        let simple_count = simple_primitives.len();
        let liquid_count = liquid_primitives.len();

        if simple_count == 0 && liquid_count == 0 {
            return;
        }

        // Combine: simple primitives first, then liquid primitives
        let mut ordered_primitives = simple_primitives;
        ordered_primitives.extend(liquid_primitives);

        // Ensure glass resources are cached (sampler is reused across frames)
        let current_size = self.viewport_size;

        // Check if we need to create or recreate the cached glass resources
        let need_new_bind_group = match &self.cached_glass {
            None => true,
            Some(cached) => cached.bind_group.is_none() || cached.bind_group_size != current_size,
        };

        if self.cached_glass.is_none() {
            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Glass Backdrop Sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });
            self.cached_glass = Some(CachedGlassResources {
                sampler,
                bind_group: None,
                bind_group_size: (0, 0),
            });
        }

        // Update glass uniforms
        let glass_uniforms = GlassUniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            time: self.time,
            _padding: 0.0,
        };
        self.queue.write_buffer(
            &self.buffers.glass_uniforms,
            0,
            bytemuck::bytes_of(&glass_uniforms),
        );

        // Update glass primitives buffer with ordered primitives
        self.queue.write_buffer(
            &self.buffers.glass_primitives,
            0,
            bytemuck::cast_slice(&ordered_primitives),
        );

        // Create or reuse glass bind group
        if need_new_bind_group {
            let cached_glass = self.cached_glass.as_ref().unwrap();
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Glass Bind Group"),
                layout: &self.bind_group_layouts.glass,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.buffers.glass_uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.buffers.glass_primitives.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(backdrop),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&cached_glass.sampler),
                    },
                ],
            });

            // Update cache
            if let Some(ref mut cached) = self.cached_glass {
                cached.bind_group = Some(bind_group);
                cached.bind_group_size = current_size;
            }
        }

        let glass_bind_group = self
            .cached_glass
            .as_ref()
            .unwrap()
            .bind_group
            .as_ref()
            .unwrap();

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Glass Render Encoder"),
            });

        // Begin render pass (load existing content)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Glass Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Keep existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render simple glass primitives with the simple_glass pipeline
            if simple_count > 0 {
                render_pass.set_pipeline(self.effect_pipelines.simple_glass.as_ref().unwrap());
                render_pass.set_bind_group(0, glass_bind_group, &[]);
                render_pass.draw(0..6, 0..simple_count as u32);
            }

            // Render liquid glass primitives with the glass pipeline
            if liquid_count > 0 {
                render_pass.set_pipeline(self.effect_pipelines.glass.as_ref().unwrap());
                render_pass.set_bind_group(0, glass_bind_group, &[]);
                render_pass.draw(
                    0..6,
                    simple_count as u32..(simple_count + liquid_count) as u32,
                );
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render primitives to a backdrop texture for glass blur sampling
    ///
    /// This renders the background primitives to a lower-resolution texture
    /// that glass primitives sample from for their blur effect.
    pub fn render_to_backdrop(
        &mut self,
        backdrop: &wgpu::TextureView,
        _backdrop_size: (u32, u32),
        batch: &PrimitiveBatch,
        has_backdrop_content: bool,
    ) {
        if batch.primitives.is_empty() {
            return;
        }

        // Use full viewport size for coordinate mapping, even though texture is smaller.
        // GPU automatically maps NDC space to the texture size, ensuring primitives
        // appear at correct relative positions for glass sampling.
        let main_uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue.write_buffer(
            &self.buffers.uniforms,
            0,
            bytemuck::bytes_of(&main_uniforms),
        );

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(&batch.primitives);

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Backdrop Render Encoder"),
            });

        // Render to backdrop texture
        {
            let backdrop_load = if has_backdrop_content {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(wgpu::Color::BLACK)
            };
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Backdrop Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: backdrop,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: backdrop_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            Self::draw_split_sdf(
                &mut render_pass,
                &self.pipelines,
                &sdf_ranges,
                false,
                self.sdf_vb_buffer(),
            );
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
        // Note: No need to restore uniforms since we're already using main_uniforms
    }

    /// Render glass frame with backdrop and glass primitives in a single encoder submission.
    /// This is more efficient than separate render calls as it reduces command buffer overhead.
    ///
    /// Performs:
    /// 1. Render background primitives to backdrop texture
    /// 2. Render background primitives to target
    /// 3. Render glass primitives with backdrop blur to target
    pub fn render_glass_frame(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        _backdrop_size: (u32, u32), // Not used - we render with full viewport coords
        batch: &PrimitiveBatch,
        has_backdrop_content: bool,
    ) {
        // Glass effects require storage buffers for per-frame primitive data.
        // On WebGL2 (no storage buffers), skip glass rendering — the glass DT
        // shader exists but needs a per-frame glass data texture + bind group
        // plumbing that isn't implemented yet.
        if !self.has_storage_buffers {
            return;
        }
        self.ensure_glass_pipelines();

        // Update uniforms for rendering (always use full viewport size)
        // The GPU maps NDC space to actual texture size automatically
        let main_uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };

        // Update auxiliary data buffer
        self.update_aux_data_buffer(batch);

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(&batch.primitives);

        // Split glass primitives into simple and liquid for separate rendering
        let mut simple_primitives: Vec<GpuGlassPrimitive> = Vec::new();
        let mut liquid_primitives: Vec<GpuGlassPrimitive> = Vec::new();
        for prim in &batch.glass_primitives {
            if prim.type_info[0] == GlassType::Simple as u32 {
                simple_primitives.push(*prim);
            } else {
                liquid_primitives.push(*prim);
            }
        }
        let simple_count = simple_primitives.len();
        let liquid_count = liquid_primitives.len();

        // Combine: simple first, then liquid
        let mut ordered_glass_primitives = simple_primitives;
        ordered_glass_primitives.extend(liquid_primitives);

        // Update glass primitives buffer with ordered primitives
        if !ordered_glass_primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.glass_primitives,
                0,
                bytemuck::cast_slice(&ordered_glass_primitives),
            );
        }

        // Update glass uniforms
        let glass_uniforms = GlassUniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            time: self.time,
            _padding: 0.0,
        };
        self.queue.write_buffer(
            &self.buffers.glass_uniforms,
            0,
            bytemuck::bytes_of(&glass_uniforms),
        );

        // Ensure glass bind group is cached
        let current_size = self.viewport_size;
        let need_new_bind_group = match &self.cached_glass {
            None => true,
            Some(cached) => cached.bind_group.is_none() || cached.bind_group_size != current_size,
        };

        if self.cached_glass.is_none() {
            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Glass Backdrop Sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });
            self.cached_glass = Some(CachedGlassResources {
                sampler,
                bind_group: None,
                bind_group_size: (0, 0),
            });
        }

        if need_new_bind_group {
            let cached_glass = self.cached_glass.as_ref().unwrap();
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Glass Bind Group"),
                layout: &self.bind_group_layouts.glass,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.buffers.glass_uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.buffers.glass_primitives.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(backdrop),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&cached_glass.sampler),
                    },
                ],
            });
            if let Some(ref mut cached) = self.cached_glass {
                cached.bind_group = Some(bind_group);
                cached.bind_group_size = current_size;
            }
        }

        // Create single command encoder for entire frame
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Glass Frame Encoder"),
            });

        // Pass 1: Render background primitives to backdrop texture (at half resolution)
        // NOTE: We use main_uniforms (full viewport size) for coordinate mapping,
        // even though the texture is half resolution. The GPU automatically maps
        // NDC space to the texture size. This ensures primitives appear at correct
        // relative positions for glass sampling.
        {
            self.queue.write_buffer(
                &self.buffers.uniforms,
                0,
                bytemuck::bytes_of(&main_uniforms),
            );

            let backdrop_load = if has_backdrop_content {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
            };
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Backdrop Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: backdrop,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: backdrop_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if !batch.primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    false,
                    self.sdf_vb_buffer(),
                );
            }
        }

        // Pass 2: Render background primitives to target (at full resolution)
        {
            self.queue.write_buffer(
                &self.buffers.uniforms,
                0,
                bytemuck::bytes_of(&main_uniforms),
            );

            let target_load = if has_backdrop_content {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(wgpu::Color::BLACK)
            };
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Target Background Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: target_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if !batch.primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    false,
                    self.sdf_vb_buffer(),
                );
            }
        }

        // Pass 3: Render glass primitives with backdrop blur
        if simple_count > 0 || liquid_count > 0 {
            let glass_bind_group = self
                .cached_glass
                .as_ref()
                .unwrap()
                .bind_group
                .as_ref()
                .unwrap();

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Glass Render Pass"),
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

            // Render simple glass primitives with simple_glass pipeline
            if simple_count > 0 {
                render_pass.set_pipeline(self.effect_pipelines.simple_glass.as_ref().unwrap());
                render_pass.set_bind_group(0, glass_bind_group, &[]);
                render_pass.draw(0..6, 0..simple_count as u32);
            }

            // Render liquid glass primitives with glass pipeline
            if liquid_count > 0 {
                render_pass.set_pipeline(self.effect_pipelines.glass.as_ref().unwrap());
                render_pass.set_bind_group(0, glass_bind_group, &[]);
                render_pass.draw(
                    0..6,
                    simple_count as u32..(simple_count + liquid_count) as u32,
                );
            }
        }

        // Submit background and glass passes first
        self.queue.submit(std::iter::once(encoder.finish()));

        // Pass 3b: Render nested glass primitives (glass inside glass)
        // These are glass elements that are children of other glass elements.
        // They render after parent glass, sampling from the same backdrop.
        if !batch.nested_glass_primitives.is_empty() {
            // Split nested glass into simple and liquid
            let mut nested_simple: Vec<GpuGlassPrimitive> = Vec::new();
            let mut nested_liquid: Vec<GpuGlassPrimitive> = Vec::new();
            for prim in &batch.nested_glass_primitives {
                if prim.type_info[0] == GlassType::Simple as u32 {
                    nested_simple.push(*prim);
                } else {
                    nested_liquid.push(*prim);
                }
            }
            let nested_simple_count = nested_simple.len();
            let nested_liquid_count = nested_liquid.len();

            // Combine: simple first, then liquid
            let mut ordered_nested = nested_simple;
            ordered_nested.extend(nested_liquid);

            // Upload nested glass primitives to buffer
            self.queue.write_buffer(
                &self.buffers.glass_primitives,
                0,
                bytemuck::cast_slice(&ordered_nested),
            );

            // Recreate bind group since glass_primitives buffer contents changed
            {
                let cached_glass = self.cached_glass.as_ref().unwrap();
                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Nested Glass Bind Group"),
                    layout: &self.bind_group_layouts.glass,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.buffers.glass_uniforms.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: self.buffers.glass_primitives.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(backdrop),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(&cached_glass.sampler),
                        },
                    ],
                });
                if let Some(ref mut cached) = self.cached_glass {
                    cached.bind_group = Some(bind_group);
                }
            }

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Blinc Nested Glass Encoder"),
                });

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Nested Glass Render Pass"),
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

            let nested_bind_group = self
                .cached_glass
                .as_ref()
                .unwrap()
                .bind_group
                .as_ref()
                .unwrap();

            if nested_simple_count > 0 {
                render_pass.set_pipeline(self.effect_pipelines.simple_glass.as_ref().unwrap());
                render_pass.set_bind_group(0, nested_bind_group, &[]);
                render_pass.draw(0..6, 0..nested_simple_count as u32);
            }

            if nested_liquid_count > 0 {
                render_pass.set_pipeline(self.effect_pipelines.glass.as_ref().unwrap());
                render_pass.set_bind_group(0, nested_bind_group, &[]);
                render_pass.draw(
                    0..6,
                    nested_simple_count as u32..(nested_simple_count + nested_liquid_count) as u32,
                );
            }

            drop(render_pass);
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // Pass 4: Render foreground primitives (on top of glass)
        // This requires a separate submission because we need to overwrite the primitives buffer
        if !batch.foreground_primitives.is_empty() {
            // Sort and upload foreground primitives
            let fg_ranges = self.upload_sorted_primitives(&batch.foreground_primitives);

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Blinc Foreground Encoder"),
                });

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Foreground Render Pass"),
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

            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            Self::draw_split_sdf(
                &mut render_pass,
                &self.pipelines,
                &fg_ranges,
                false,
                self.sdf_vb_buffer(),
            );

            drop(render_pass);
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // Pass 5: Render paths (SVGs) on top of glass
        // Paths are tessellated geometry that need their own pipeline
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            // Update path buffers (creates/resizes as needed)
            self.update_path_buffers(batch);

            // Render paths
            if let (Some(vb), Some(ib)) = (&self.buffers.path_vertices, &self.buffers.path_indices)
            {
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Blinc Glass Path Encoder"),
                        });

                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Glass Path Render Pass"),
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

                // Use overlay path pipeline (1x sampled, no MSAA)
                render_pass.set_pipeline(&self.pipelines.path_overlay);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);

                drop(render_pass);
                self.queue.submit(std::iter::once(encoder.finish()));
            }
        }
    }

    /// Render primitives as an overlay on existing content (1x sampled)
    ///
    /// This uses the overlay pipeline which is configured for sample_count=1,
    /// making it suitable for rendering on top of already-resolved content
    /// (e.g., after glass effects have been applied).
    ///
    /// # Arguments
    /// * `target` - The single-sampled texture view to render to (existing content is preserved)
    /// * `batch` - The primitive batch to render
    pub fn render_overlay(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        // Phase 4a: include pure-opacity layers in the gate so an
        // overlay containing only opacity-animated content takes the
        // layer-aware path (which actually composites with the
        // configured opacity) instead of the simple path (which drops
        // layer commands entirely).
        let has_layer_processing = batch.layer_commands.iter().any(|entry| {
            if let crate::primitives::LayerCommand::Push { config } = &entry.command {
                !config.effects.is_empty()
                    || config.blend_mode != blinc_core::BlendMode::Normal
                    || config.opacity < 1.0
            } else {
                false
            }
        });

        // If we have layer effects or blend modes, use the layer-aware rendering path
        if has_layer_processing {
            self.render_overlay_with_layer_effects(target, batch);
            return;
        }

        // Standard overlay rendering (no layer effects)
        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update auxiliary data buffer
        self.update_aux_data_buffer(batch);

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(&batch.primitives);

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Overlay Render Encoder"),
            });

        // Begin render pass (load existing content, don't clear)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Overlay Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None, // No MSAA resolve needed for overlay
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Keep existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render paths first (they're typically backgrounds)
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(&self.pipelines.path_overlay);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
                }
            }

            // Render SDF primitives using split overlay pipelines
            if !batch.primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    true,
                    self.sdf_vb_buffer(),
                );
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render overlay with layer effect/blend-mode processing
    ///
    /// Follows the same pattern as render_with_layer_effects:
    /// 1. Build list of layers that need processing (effects or blend modes)
    /// 2. Render non-layer primitives normally (overlay = LoadOp::Load)
    /// 3. For each layer, render to tight offscreen texture, apply effects, blit at position
    fn render_overlay_with_layer_effects(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
    ) {
        use crate::primitives::LayerCommand;

        // Build list of layers with their primitive ranges
        let mut effect_layers: Vec<(usize, usize, blinc_core::LayerConfig)> = Vec::new();
        let mut layer_stack: Vec<(usize, blinc_core::LayerConfig)> = Vec::new();

        for entry in &batch.layer_commands {
            match &entry.command {
                LayerCommand::Push { config } => {
                    layer_stack.push((entry.primitive_index, config.clone()));
                }
                LayerCommand::Pop => {
                    if let Some((start_idx, config)) = layer_stack.pop() {
                        // Phase 4a: pure-opacity layers also need
                        // processing so `config.opacity` reaches the
                        // composite blit instead of being silently
                        // dropped.
                        let needs_processing = !config.effects.is_empty()
                            || config.blend_mode != blinc_core::BlendMode::Normal
                            || config.transform_3d.is_some()
                            || config.opacity < 1.0;
                        if needs_processing {
                            effect_layers.push((start_idx, entry.primitive_index, config));
                        }
                    }
                }
                LayerCommand::Sample { .. } => {}
            }
        }

        if effect_layers.is_empty() {
            self.render_overlay_simple(target, batch);
            return;
        }

        // Build set of primitive indices that belong to effect/blend layers (skip in first pass)
        let mut effect_primitives = std::collections::HashSet::new();
        for (start, end, _) in &effect_layers {
            for i in *start..*end {
                effect_primitives.insert(i);
            }
        }

        // First pass: render primitives NOT in effect/blend layers (overlay = Load)
        self.render_overlay_primitives_excluding(target, batch, &effect_primitives);
        drop(effect_primitives);

        // Process each effect/blend layer
        for (start_idx, end_idx, config) in effect_layers {
            if start_idx >= end_idx || end_idx > batch.primitives.len() {
                continue;
            }

            // Compute bounding box from primitives (screen coordinates)
            let primitives = &batch.primitives[start_idx..end_idx];
            let (layer_pos, layer_size, layer_clip) = if primitives.is_empty() {
                let pos = config.position.map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
                let size = config
                    .size
                    .map(|s| (s.width, s.height))
                    .unwrap_or((self.viewport_size.0 as f32, self.viewport_size.1 as f32));
                (pos, size, None)
            } else {
                let mut min_x = f32::MAX;
                let mut min_y = f32::MAX;
                let mut max_x = f32::MIN;
                let mut max_y = f32::MIN;
                let mut clip: Option<([f32; 4], [f32; 4])> = None;
                for p in primitives {
                    let (px, py, pw, ph) = (p.bounds[0], p.bounds[1], p.bounds[2], p.bounds[3]);
                    min_x = min_x.min(px);
                    min_y = min_y.min(py);
                    max_x = max_x.max(px + pw);
                    max_y = max_y.max(py + ph);
                    if clip.is_none() && p.clip_bounds[0] > -5000.0 && p.clip_bounds[2] < 90000.0 {
                        clip = Some((p.clip_bounds, p.clip_radius));
                    }
                }
                let width = (max_x - min_x).max(1.0);
                let height = (max_y - min_y).max(1.0);
                ((min_x, min_y), (width, height), clip)
            };

            // Skip layers entirely outside the viewport
            let vp_w = self.viewport_size.0 as f32;
            let vp_h = self.viewport_size.1 as f32;
            let is_visible = layer_pos.0 < vp_w
                && layer_pos.1 < vp_h
                && layer_pos.0 + layer_size.0 > 0.0
                && layer_pos.1 + layer_size.1 > 0.0
                && layer_size.0 > 0.0
                && layer_size.1 > 0.0;

            if !is_visible {
                continue;
            }

            let effect_expansion = Self::calculate_effect_expansion(&config.effects);

            // Render layer primitives to tight texture with offset
            let (layer_texture, content_size) = self.render_primitive_range_tight(
                batch,
                start_idx,
                end_idx,
                layer_pos,
                layer_size,
                effect_expansion,
            );

            let tight_size = content_size;
            let expanded_pos = (
                layer_pos.0 - effect_expansion.0,
                layer_pos.1 - effect_expansion.1,
            );
            let expanded_size = (
                layer_size.0 + effect_expansion.0 + effect_expansion.2,
                layer_size.1 + effect_expansion.1 + effect_expansion.3,
            );

            if config.effects.is_empty() {
                // Blend-mode only: blit directly
                self.blit_tight_texture_to_target(
                    &layer_texture.view,
                    tight_size,
                    target,
                    expanded_pos,
                    expanded_size,
                    config.opacity,
                    config.blend_mode,
                    layer_clip,
                    config.transform_3d,
                );
                self.layer_texture_cache.release(layer_texture);
            } else {
                // Apply effects then blit
                let effected = self.apply_layer_effects(&layer_texture, &config.effects);
                self.layer_texture_cache.release(layer_texture);

                self.blit_tight_texture_to_target(
                    &effected.view,
                    tight_size,
                    target,
                    expanded_pos,
                    expanded_size,
                    config.opacity,
                    config.blend_mode,
                    layer_clip,
                    config.transform_3d,
                );
                self.layer_texture_cache.release(effected);
            }
        }
    }

    /// Render overlay primitives excluding those in the given set (LoadOp::Load)
    fn render_overlay_primitives_excluding(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        exclude: &std::collections::HashSet<usize>,
    ) {
        if exclude.is_empty() {
            self.render_overlay_simple(target, batch);
            return;
        }

        let included_primitives: Vec<GpuPrimitive> = batch
            .primitives
            .iter()
            .enumerate()
            .filter(|(i, _)| !exclude.contains(i))
            .map(|(_, p)| *p)
            .collect();

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update auxiliary data buffer
        self.update_aux_data_buffer(batch);

        // Update path buffers
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Sort and upload filtered primitives
        let sdf_ranges = self.upload_sorted_primitives(&included_primitives);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Overlay Excluding Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Overlay Excluding Pass"),
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

            // Render paths first
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(&self.pipelines.path_overlay);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
                }
            }

            // Render filtered SDF primitives via split overlay pipelines
            if !included_primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    true,
                    self.sdf_vb_buffer(),
                );
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Simple overlay render without layer effect processing
    fn render_overlay_simple(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(&batch.primitives);

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Overlay Simple Render Encoder"),
            });

        // Begin render pass (load existing content, don't clear)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Overlay Simple Render Pass"),
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

            // Render paths first
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(&self.pipelines.path_overlay);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
                }
            }

            // Render SDF primitives via split overlay pipelines
            if !batch.primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    true,
                    self.sdf_vb_buffer(),
                );
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a slice of primitives as overlay (LoadOp::Load, keeps existing content)
    ///
    /// This is used for interleaved z-layer rendering where primitives need
    /// to be rendered per-layer to properly interleave with text.
    /// Uses `self.bind_groups.sdf` which automatically includes the real glyph
    /// atlas when `set_glyph_atlas()` was called at the start of the frame.
    pub fn render_primitives_overlay(
        &mut self,
        target: &wgpu::TextureView,
        primitives: &[GpuPrimitive],
    ) {
        if primitives.is_empty() {
            return;
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(primitives);

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Layer Primitives Encoder"),
            });

        // Begin render pass (load existing content)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Layer Primitives Pass"),
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

            // Render SDF primitives via split overlay pipelines
            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            Self::draw_split_sdf(
                &mut render_pass,
                &self.pipelines,
                &sdf_ranges,
                true,
                self.sdf_vb_buffer(),
            );
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render paths (tessellated geometry like SVGs) as an overlay
    ///
    /// This renders paths on top of existing content without clearing.
    /// Used for z-layered rendering where paths need to be rendered separately.
    pub fn render_paths_overlay(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if !has_paths {
            return;
        }

        // Update path buffers
        self.update_path_buffers(batch);

        // Render paths
        if let (Some(vb), Some(ib)) = (&self.buffers.path_vertices, &self.buffers.path_indices) {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Blinc Paths Overlay Encoder"),
                });

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Paths Overlay Pass"),
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

            // Use overlay path pipeline (1x sampled)
            render_pass.set_pipeline(&self.pipelines.path_overlay);
            render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
            render_pass.set_vertex_buffer(0, vb.slice(..));
            render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);

            drop(render_pass);
            self.queue.submit(std::iter::once(encoder.finish()));
        }
    }

    /// Render SDF primitives with unified text rendering (text as primitives)
    ///
    /// This method renders SDF primitives including text glyphs in a single pass.
    /// Text primitives (PrimitiveType::Text) sample from the provided glyph atlases.
    /// Uses `set_glyph_atlas()` to bind the real atlas, then delegates to
    /// `render_primitives_overlay()`.
    pub fn render_primitives_overlay_with_glyphs(
        &mut self,
        target: &wgpu::TextureView,
        primitives: &[GpuPrimitive],
        atlas_view: &wgpu::TextureView,
        color_atlas_view: &wgpu::TextureView,
    ) {
        self.set_glyph_atlas(atlas_view, color_atlas_view);
        self.render_primitives_overlay(target, primitives);
    }

    /// Render overlay primitives with MSAA anti-aliasing
    ///
    /// This method renders paths/primitives to a temporary MSAA texture,
    /// resolves it, and then blends onto the target. This provides smooth
    /// edges for tessellated paths that don't have shader-based AA.
    ///
    /// # Arguments
    /// * `target` - The single-sampled texture view to render to (existing content is preserved)
    /// * `batch` - The primitive batch to render
    /// * `sample_count` - MSAA sample count (typically 4)
    pub fn render_overlay_msaa(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        sample_count: u32,
    ) {
        if batch.paths.vertices.is_empty() && batch.primitives.is_empty() {
            return;
        }

        // Ensure we have MSAA pipelines for this sample count
        let need_new_pipelines = match &self.msaa_pipelines {
            Some(p) => p.sample_count != sample_count,
            None => true,
        };
        if need_new_pipelines && sample_count > 1 {
            self.msaa_pipelines = Some(Self::create_msaa_pipelines(
                &self.device,
                &self.bind_group_layouts,
                self.texture_format,
                sample_count,
                self.has_vertex_storage,
                self.has_storage_buffers,
            ));
        }

        let (width, height) = self.viewport_size;

        // Check if we need to recreate cached MSAA textures
        let need_new_textures = match &self.cached_msaa {
            Some(cached) => {
                cached.width != width
                    || cached.height != height
                    || cached.sample_count != sample_count
            }
            None => true,
        };

        if need_new_textures {
            // Create MSAA texture for rendering
            let msaa_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Overlay MSAA Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: self.texture_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let msaa_view = msaa_texture.create_view(&wgpu::TextureViewDescriptor::default());

            // Create resolve texture
            let resolve_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Overlay Resolve Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.texture_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let resolve_view = resolve_texture.create_view(&wgpu::TextureViewDescriptor::default());

            // Create sampler (reused across frames)
            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Overlay Blend Sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });

            // Create composite uniforms (opacity=1.0, blend_mode=normal)
            #[repr(C)]
            #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
            struct CompositeUniforms {
                opacity: f32,
                blend_mode: u32,
                _padding: [f32; 2],
            }
            let composite_uniforms = CompositeUniforms {
                opacity: 1.0,
                blend_mode: 0,
                _padding: [0.0; 2],
            };
            let composite_uniform_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Composite Uniforms Buffer"),
                        contents: bytemuck::bytes_of(&composite_uniforms),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });

            // Create bind group for compositing
            let composite_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Overlay Composite Bind Group"),
                layout: &self.bind_group_layouts.composite,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: composite_uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&resolve_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });

            self.cached_msaa = Some(CachedMsaaTextures {
                msaa_texture,
                msaa_view,
                resolve_texture,
                resolve_view,
                width,
                height,
                sample_count,
                sampler,
                composite_uniform_buffer,
                composite_bind_group,
            });
        }

        // Update uniforms. `_padding.x = 1.0` signals the SDF fragment
        // shader that hardware multisample coverage is driving
        // silhouette AA for this pass — see `render_primitives_overlay_msaa`
        // for the longer rationale on why stacking the shader's
        // per-edge fade on top of MSAA resolve under-fills every
        // partially-covered pixel. The flag is only honoured by the
        // mesh-primitive branch inside `sdf_core`; every other branch
        // ignores `_padding`.
        let msaa_flag = if sample_count > 1 { 1.0 } else { 0.0 };
        let uniforms = Uniforms {
            viewport_size: [width as f32, height as f32],
            _padding: [msaa_flag, 0.0],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Sort and upload primitives
        let sdf_ranges = self.upload_sorted_primitives(&batch.primitives);

        // Upload auxiliary data buffer so PRIM_MESH triangle corners and
        // polygon-clip vertices resolve to live GPU data. The
        // single-sampled overlay path piggybacks on the upload that
        // `render_with_clear_simple` does at the top of the frame, but
        // callers that enter this MSAA path *in place of*
        // `render_with_clear` skip that upload and the GPU buffer then
        // carries stale data from the previous frame — every mesh
        // triangle indexes into random `aux_data` and either renders as
        // nothing or as a degenerate sliver. Updating here keeps the
        // MSAA overlay self-sufficient.
        self.update_aux_data_buffer(batch);

        // Update path buffers
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Get references to the cached textures (after mutable borrows are done)
        let cached = self.cached_msaa.as_ref().unwrap();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Overlay MSAA Render Encoder"),
            });

        // Pass 1: Render paths + SDF primitives to an offscreen texture.
        //
        // WebGPU spec: `resolveTarget` MUST be None when the color
        // attachment's sample_count is 1. Chrome/Dawn silently accept
        // a stray `resolve_target: Some(...)` on a single-sampled
        // view, but Safari/WebKit rejects the render pass entirely —
        // which is why every path draw (notches, SVG strokes, custom
        // paths) was invisible on Safari.
        //
        // Fix: when sample_count == 1, render directly into
        // `resolve_view` (a single-sampled texture with both
        // RENDER_ATTACHMENT and TEXTURE_BINDING usage) and pass
        // `resolve_target: None`. When sample_count > 1, use the
        // multisampled `msaa_view` with a resolve into `resolve_view`
        // as before.
        let (pass1_view, pass1_resolve, pass1_store) = if sample_count > 1 {
            // Multisampled: keep the resolved single-sample texture,
            // discard the MSAA texture (we only wanted its resolved
            // output, not the per-sample content).
            (
                &cached.msaa_view,
                Some(&cached.resolve_view),
                wgpu::StoreOp::Discard,
            )
        } else {
            // Single-sampled: render directly into the resolve_view
            // and keep the content so pass 2 can sample from it.
            (&cached.resolve_view, None, wgpu::StoreOp::Store)
        };
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay MSAA Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: pass1_view,
                    resolve_target: pass1_resolve,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: pass1_store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Get the appropriate path pipeline for the sample count
            let path_pipeline = if sample_count > 1 {
                if let Some(ref msaa) = self.msaa_pipelines {
                    &msaa.path
                } else {
                    &self.pipelines.path
                }
            } else {
                &self.pipelines.path
            };

            // Render paths using MSAA pipeline
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(path_pipeline);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
                }
            }

            // Render SDF primitives using split MSAA pipelines
            if !batch.primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                if sample_count > 1 {
                    if let Some(ref msaa) = self.msaa_pipelines {
                        Self::draw_split_sdf_msaa(
                            &mut render_pass,
                            msaa,
                            &sdf_ranges,
                            self.sdf_vb_buffer(),
                        );
                    } else {
                        Self::draw_split_sdf(
                            &mut render_pass,
                            &self.pipelines,
                            &sdf_ranges,
                            false,
                            self.sdf_vb_buffer(),
                        );
                    }
                } else {
                    Self::draw_split_sdf(
                        &mut render_pass,
                        &self.pipelines,
                        &sdf_ranges,
                        false,
                        self.sdf_vb_buffer(),
                    );
                }
            }
        }

        // Pass 2: Blend resolved texture onto target using cached resources
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay Blend Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Keep existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.composite_overlay);
            render_pass.set_bind_group(0, &cached.composite_bind_group, &[]);
            render_pass.draw(0..3, 0..1); // Fullscreen triangle
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render only paths with MSAA anti-aliasing
    ///
    /// This is used when SDF primitives are rendered separately (unified rendering mode)
    /// but paths still need MSAA for smooth edges.
    pub fn render_paths_overlay_msaa(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        sample_count: u32,
    ) {
        if batch.paths.vertices.is_empty() || batch.paths.indices.is_empty() {
            return;
        }

        // Ensure we have MSAA pipelines for this sample count
        let need_new_pipelines = match &self.msaa_pipelines {
            Some(p) => p.sample_count != sample_count,
            None => true,
        };
        if need_new_pipelines && sample_count > 1 {
            self.msaa_pipelines = Some(Self::create_msaa_pipelines(
                &self.device,
                &self.bind_group_layouts,
                self.texture_format,
                sample_count,
                self.has_vertex_storage,
                self.has_storage_buffers,
            ));
        }

        let (width, height) = self.viewport_size;

        // Check if we need to recreate cached MSAA textures
        let need_new_textures = match &self.cached_msaa {
            Some(cached) => {
                cached.width != width
                    || cached.height != height
                    || cached.sample_count != sample_count
            }
            None => true,
        };

        if need_new_textures {
            // Create MSAA texture for rendering
            let msaa_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Path MSAA Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: self.texture_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let msaa_view = msaa_texture.create_view(&wgpu::TextureViewDescriptor::default());

            // Create resolve texture
            let resolve_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Path Resolve Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.texture_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let resolve_view = resolve_texture.create_view(&wgpu::TextureViewDescriptor::default());

            // Create sampler
            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Path Blend Sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });

            // Create composite uniforms
            #[repr(C)]
            #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
            struct CompositeUniforms {
                opacity: f32,
                blend_mode: u32,
                _padding: [f32; 2],
            }
            let composite_uniforms = CompositeUniforms {
                opacity: 1.0,
                blend_mode: 0,
                _padding: [0.0; 2],
            };
            let composite_uniform_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Path Composite Uniforms Buffer"),
                        contents: bytemuck::bytes_of(&composite_uniforms),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });

            // Create bind group for compositing
            let composite_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Path Composite Bind Group"),
                layout: &self.bind_group_layouts.composite,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: composite_uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&resolve_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });

            self.cached_msaa = Some(CachedMsaaTextures {
                msaa_texture,
                msaa_view,
                resolve_texture,
                resolve_view,
                width,
                height,
                sample_count,
                sampler,
                composite_uniform_buffer,
                composite_bind_group,
            });
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [width as f32, height as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update path buffers
        self.update_path_buffers(batch);

        // Get references to the cached textures
        let cached = self.cached_msaa.as_ref().unwrap();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Path MSAA Render Encoder"),
            });

        // Pass 1: Render paths to an offscreen texture.
        //
        // See the longer comment in `render_overlay_msaa` — the key
        // constraint is that `resolveTarget` must be None when the
        // color attachment is single-sampled (WebGPU spec). Safari
        // enforces this; Chrome accepts a stray `Some(...)` silently.
        let (pass1_view, pass1_resolve, pass1_store) = if sample_count > 1 {
            (
                &cached.msaa_view,
                Some(&cached.resolve_view),
                wgpu::StoreOp::Discard,
            )
        } else {
            (&cached.resolve_view, None, wgpu::StoreOp::Store)
        };
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Path MSAA Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: pass1_view,
                    resolve_target: pass1_resolve,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: pass1_store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Get the appropriate pipeline for the sample count
            let path_pipeline = if sample_count > 1 {
                if let Some(ref msaa) = self.msaa_pipelines {
                    &msaa.path
                } else {
                    &self.pipelines.path
                }
            } else {
                &self.pipelines.path
            };

            if let (Some(vb), Some(ib)) = (&self.buffers.path_vertices, &self.buffers.path_indices)
            {
                render_pass.set_pipeline(path_pipeline);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
            }
        }

        // Pass 2: Blend resolved texture onto target
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Path Blend Pass"),
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

            render_pass.set_pipeline(&self.pipelines.composite_overlay);
            render_pass.set_bind_group(0, &cached.composite_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a slice of SDF primitives with MSAA anti-aliasing.
    ///
    /// Mirrors [`Self::render_primitives_overlay`] but dispatches through
    /// the MSAA-enabled split SDF pipelines so mesh primitives
    /// (`PRIM_MESH` — tessellated path solid fills) and the other SDF
    /// categories get the same hardware-resolved smoothing that paths
    /// and gradients already receive via
    /// [`Self::render_paths_overlay_msaa`].
    ///
    /// Without this, unified-text mode routed solid path fills through
    /// `render_unified` (single-sampled) and only the gradient / stroke
    /// paths took the MSAA overlay, which left vector animation output
    /// visibly rougher than the rasterized-SVG path even though both
    /// tessellate cubic Beziers at the same tolerance.
    ///
    /// Pattern matches `render_paths_overlay_msaa`: render into the
    /// cached MSAA texture (or directly into the single-sampled resolve
    /// view when `sample_count == 1`, required by WebGPU spec so Safari
    /// doesn't reject the pass), then blend the resolved texture onto
    /// `target` via the composite overlay pipeline.
    pub fn render_primitives_overlay_msaa(
        &mut self,
        target: &wgpu::TextureView,
        primitives: &[GpuPrimitive],
        sample_count: u32,
    ) {
        if primitives.is_empty() {
            return;
        }

        // Ensure MSAA pipelines exist at the requested sample count.
        let need_new_pipelines = match &self.msaa_pipelines {
            Some(p) => p.sample_count != sample_count,
            None => true,
        };
        if need_new_pipelines && sample_count > 1 {
            self.msaa_pipelines = Some(Self::create_msaa_pipelines(
                &self.device,
                &self.bind_group_layouts,
                self.texture_format,
                sample_count,
                self.has_vertex_storage,
                self.has_storage_buffers,
            ));
        }

        let (width, height) = self.viewport_size;

        // Reuse the shared MSAA texture cache used by the paths / overlay
        // MSAA paths. When the viewport or sample count changes we rebuild
        // it; otherwise every MSAA pass this frame shares the same
        // texture pair + composite bind group.
        let need_new_textures = match &self.cached_msaa {
            Some(cached) => {
                cached.width != width
                    || cached.height != height
                    || cached.sample_count != sample_count
            }
            None => true,
        };

        if need_new_textures {
            let msaa_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Primitives MSAA Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: self.texture_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let msaa_view = msaa_texture.create_view(&wgpu::TextureViewDescriptor::default());

            let resolve_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Primitives Resolve Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.texture_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let resolve_view = resolve_texture.create_view(&wgpu::TextureViewDescriptor::default());

            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Primitives Blend Sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });

            #[repr(C)]
            #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
            struct CompositeUniforms {
                opacity: f32,
                blend_mode: u32,
                _padding: [f32; 2],
            }
            let composite_uniforms = CompositeUniforms {
                opacity: 1.0,
                blend_mode: 0,
                _padding: [0.0; 2],
            };
            let composite_uniform_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Primitives Composite Uniforms Buffer"),
                        contents: bytemuck::bytes_of(&composite_uniforms),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });

            let composite_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Primitives Composite Bind Group"),
                layout: &self.bind_group_layouts.composite,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: composite_uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&resolve_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });

            self.cached_msaa = Some(CachedMsaaTextures {
                msaa_texture,
                msaa_view,
                resolve_texture,
                resolve_view,
                width,
                height,
                sample_count,
                sampler,
                composite_uniform_buffer,
                composite_bind_group,
            });
        }

        // Update viewport uniform for this pass.
        //
        // `_padding.x` carries an MSAA flag: 1.0 here tells the SDF
        // fragment shader that hardware multisample coverage is driving
        // silhouette AA, so the mesh-primitive branch should skip its
        // own per-edge barycentric fade. The fade works well on
        // single-sampled targets (web fallback) but stacks with MSAA
        // resolve to under-fill partially-covered pixels — a pixel with
        // 50 % coverage resolves to ~0.05 alpha instead of 0.5 because
        // each rendered sample already carries a faded-down value
        // before the average. Removing the redundant shader fade
        // restores the resolve to hardware-accurate coverage.
        let msaa_flag = if sample_count > 1 { 1.0 } else { 0.0 };
        let uniforms = Uniforms {
            viewport_size: [width as f32, height as f32],
            _padding: [msaa_flag, 0.0],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Sort + upload primitives into the shared instance / aux buffers
        // exactly the way `render_primitives_overlay` does.
        let sdf_ranges = self.upload_sorted_primitives(primitives);

        let cached = self.cached_msaa.as_ref().unwrap();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Primitives MSAA Render Encoder"),
            });

        // Pass 1: draw into MSAA (or single-sampled resolve) texture.
        // WebGPU: `resolve_target` must be None when the color
        // attachment is single-sampled, so branch on `sample_count`.
        let (pass1_view, pass1_resolve, pass1_store) = if sample_count > 1 {
            (
                &cached.msaa_view,
                Some(&cached.resolve_view),
                wgpu::StoreOp::Discard,
            )
        } else {
            (&cached.resolve_view, None, wgpu::StoreOp::Store)
        };
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Primitives MSAA Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: pass1_view,
                    resolve_target: pass1_resolve,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: pass1_store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            if sample_count > 1 {
                if let Some(ref msaa) = self.msaa_pipelines {
                    Self::draw_split_sdf_msaa(
                        &mut render_pass,
                        msaa,
                        &sdf_ranges,
                        self.sdf_vb_buffer(),
                    );
                } else {
                    Self::draw_split_sdf(
                        &mut render_pass,
                        &self.pipelines,
                        &sdf_ranges,
                        true,
                        self.sdf_vb_buffer(),
                    );
                }
            } else {
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    true,
                    self.sdf_vb_buffer(),
                );
            }
        }

        // Pass 2: composite the resolved MSAA texture over `target`,
        // preserving existing content (LoadOp::Load).
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Primitives Blend Pass"),
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

            render_pass.set_pipeline(&self.pipelines.composite_overlay);
            render_pass.set_bind_group(0, &cached.composite_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render text glyphs with a provided atlas texture
    ///
    /// # Arguments
    /// * `target` - The texture view to render to
    /// * `glyphs` - The glyph instances to render
    /// * `atlas_view` - The grayscale glyph atlas texture view
    /// * `color_atlas_view` - The color (RGBA) glyph atlas texture view for emoji
    /// * `atlas_sampler` - The sampler for the atlases
    pub fn render_text(
        &mut self,
        target: &wgpu::TextureView,
        glyphs: &[GpuGlyph],
        atlas_view: &wgpu::TextureView,
        color_atlas_view: &wgpu::TextureView,
        atlas_sampler: &wgpu::Sampler,
    ) {
        if glyphs.is_empty() {
            return;
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update glyphs: storage buffer or data texture
        if self.has_storage_buffers {
            self.queue
                .write_buffer(&self.buffers.glyphs, 0, bytemuck::cast_slice(glyphs));
        } else if let Some(ref tex) = self.buffers.glyph_data_texture {
            if !glyphs.is_empty() {
                let bytes = bytemuck::cast_slice(glyphs);
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    bytes,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(6 * 16), // 6 texels × 16 bytes per Rgba32Float = 96 bytes = sizeof(GpuGlyph)
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width: 6,
                        height: glyphs.len() as u32,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        // Check if we need to recreate the text bind group
        // Invalidate if either atlas view pointer changed (texture was recreated)
        let atlas_view_ptr = atlas_view as *const wgpu::TextureView;
        let color_atlas_view_ptr = color_atlas_view as *const wgpu::TextureView;
        let need_new_bind_group = match &self.cached_text {
            Some(cached) => {
                cached.atlas_view_ptr != atlas_view_ptr
                    || cached.color_atlas_view_ptr != color_atlas_view_ptr
            }
            None => true,
        };

        if need_new_bind_group {
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Text Bind Group"),
                layout: &self.bind_group_layouts.text,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.buffers.uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: if self.has_storage_buffers {
                            self.buffers.glyphs.as_entire_binding()
                        } else {
                            wgpu::BindingResource::TextureView(
                                self.buffers.glyph_data_view.as_ref().unwrap(),
                            )
                        },
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(atlas_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(atlas_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(color_atlas_view),
                    },
                ],
            });
            self.cached_text = Some(CachedTextResources {
                bind_group,
                atlas_view_ptr,
                color_atlas_view_ptr,
            });
        }

        let text_bind_group = &self.cached_text.as_ref().unwrap().bind_group;

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Text Render Encoder"),
            });

        // Begin render pass (load existing content)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Text Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Keep existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Use text_overlay pipeline since we're rendering to 1x sampled texture
            render_pass.set_pipeline(&self.pipelines.text_overlay);
            render_pass.set_bind_group(0, text_bind_group, &[]);
            // Apply staged scissor (compositor v2 damage-rect path)
            // so this dispatch only paints into the same rect that
            // the SDF clear + redraw covered. Without this guard a
            // text glyph emitted into a motion-bound subtree would
            // paint outside the damaged region and stick on top of
            // the static cache's previous-frame pixels.
            if let Some((sx, sy, sw, sh)) = self.pending_scissor {
                render_pass.set_scissor_rect(sx, sy, sw, sh);
            }
            render_pass.draw(0..6, 0..glyphs.len() as u32);
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Create the image rendering pipeline (lazily initialized)
    fn ensure_image_pipeline(&mut self) {
        if self.image_pipeline.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Image Shader"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(IMAGE_SHADER)),
            });

        // Bind group layout: uniforms, texture, sampler
        let bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Image Bind Group Layout"),
                    entries: &[
                        // Uniforms (viewport size)
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // Image texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // Sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Image Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        // Blending for premultiplied alpha
        let blend_state = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Image Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GpuImageInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            // dst_rect
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 0,
                                shader_location: 0,
                            },
                            // src_uv
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 16,
                                shader_location: 1,
                            },
                            // tint
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 32,
                                shader_location: 2,
                            },
                            // params (border_radius, opacity, border_width, packed_border_color)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 48,
                                shader_location: 3,
                            },
                            // clip_bounds (x, y, width, height)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 64,
                                shader_location: 4,
                            },
                            // clip_radius (tl, tr, br, bl)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 80,
                                shader_location: 5,
                            },
                            // filter_a (grayscale, invert, sepia, hue_rotate_rad)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 96,
                                shader_location: 6,
                            },
                            // filter_b (brightness, contrast, saturate, unused)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 112,
                                shader_location: 7,
                            },
                            // transform (a, b, c, d) - 2x2 affine matrix
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 128,
                                shader_location: 8,
                            },
                            // clip2_bounds (x, y, width, height) - secondary sharp clip
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 144,
                                shader_location: 9,
                            },
                            // mask_params (gradient geometry)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 160,
                                shader_location: 10,
                            },
                            // mask_info (type, start_alpha, end_alpha, 0)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 176,
                                shader_location: 11,
                            },
                        ],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.texture_format,
                        blend: Some(blend_state),
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

        // Create instance buffer (max 1000 images per batch)
        let instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Image Instance Buffer"),
            size: (std::mem::size_of::<GpuImageInstance>() * 1000) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create sampler
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Image Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        self.image_pipeline = Some(ImagePipeline {
            pipeline,
            bind_group_layout,
            instance_buffer,
            sampler,
        });
    }

    /// Lazily create the blur effect pipeline and its uniform buffers
    fn ensure_blur_pipeline(&mut self) {
        if self.effect_pipelines.blur.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Blur Effect Shader"),
                source: wgpu::ShaderSource::Wgsl(BLUR_SHADER.into()),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Blur Effect Pipeline Layout"),
                bind_group_layouts: &[&self.bind_group_layouts.blur],
                push_constant_ranges: &[],
            });

        let targets = &[Some(wgpu::ColorTargetState {
            format: self.texture_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        self.effect_pipelines.blur = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Blur Effect Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_kawase_blur"),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));

        // Also create the 8 uniform buffers for multi-pass blur
        if self.buffers.blur_uniforms_pool.is_none() {
            self.buffers.blur_uniforms_pool = Some(
                (0..8)
                    .map(|i| {
                        self.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some(&format!("Blur Uniforms Pass {i}")),
                            size: std::mem::size_of::<BlurUniforms>() as u64,
                            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        })
                    })
                    .collect(),
            );
        }
    }

    /// Lazily create the color matrix effect pipeline and its uniform buffer
    fn ensure_color_matrix_pipeline(&mut self) {
        if self.effect_pipelines.color_matrix.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Color Matrix Effect Shader"),
                source: wgpu::ShaderSource::Wgsl(COLOR_MATRIX_SHADER.into()),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Color Matrix Effect Pipeline Layout"),
                bind_group_layouts: &[&self.bind_group_layouts.color_matrix],
                push_constant_ranges: &[],
            });

        let targets = &[Some(wgpu::ColorTargetState {
            format: self.texture_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        self.effect_pipelines.color_matrix = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Color Matrix Effect Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_color_matrix"),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));

        if self.buffers.color_matrix_uniforms.is_none() {
            self.buffers.color_matrix_uniforms =
                Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Color Matrix Uniforms Buffer"),
                    size: std::mem::size_of::<ColorMatrixUniforms>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
        }
    }

    /// Lazily create the drop shadow effect pipeline and its uniform buffer
    fn ensure_drop_shadow_pipeline(&mut self) {
        if self.effect_pipelines.drop_shadow.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Drop Shadow Effect Shader"),
                source: wgpu::ShaderSource::Wgsl(DROP_SHADOW_SHADER.into()),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Drop Shadow Effect Pipeline Layout"),
                bind_group_layouts: &[&self.bind_group_layouts.drop_shadow],
                push_constant_ranges: &[],
            });

        let targets = &[Some(wgpu::ColorTargetState {
            format: self.texture_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        self.effect_pipelines.drop_shadow = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Drop Shadow Effect Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_drop_shadow"),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));

        if self.buffers.drop_shadow_uniforms.is_none() {
            self.buffers.drop_shadow_uniforms =
                Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Drop Shadow Uniforms Buffer"),
                    size: std::mem::size_of::<DropShadowUniforms>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
        }
    }

    /// Lazily create the glow effect pipeline and its uniform buffer
    fn ensure_glow_pipeline(&mut self) {
        if self.effect_pipelines.glow.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Glow Effect Shader"),
                source: wgpu::ShaderSource::Wgsl(GLOW_SHADER.into()),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Glow Effect Pipeline Layout"),
                bind_group_layouts: &[&self.bind_group_layouts.glow],
                push_constant_ranges: &[],
            });

        let targets = &[Some(wgpu::ColorTargetState {
            format: self.texture_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        self.effect_pipelines.glow = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Glow Effect Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_glow"),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));

        if self.buffers.glow_uniforms.is_none() {
            self.buffers.glow_uniforms = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Glow Uniforms Buffer"),
                size: std::mem::size_of::<GlowUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
    }

    /// Lazily create the mask image effect pipeline
    fn ensure_mask_image_pipeline(&mut self) {
        if self.effect_pipelines.mask_image.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Mask Image Shader"),
                source: wgpu::ShaderSource::Wgsl(MASK_IMAGE_SHADER.into()),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Mask Image Effect Pipeline Layout"),
                bind_group_layouts: &[&self.bind_group_layouts.mask_image],
                push_constant_ranges: &[],
            });

        let targets = &[Some(wgpu::ColorTargetState {
            format: self.texture_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];

        self.effect_pipelines.mask_image = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Mask Image Effect Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_mask"),
                    targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));
    }

    /// Lazily create both glass pipelines (liquid glass + simple frosted glass)
    fn ensure_glass_pipelines(&mut self) {
        if self.effect_pipelines.glass.is_some() {
            return;
        }

        let glass_source = if self.has_storage_buffers {
            GLASS_SHADER
        } else {
            GLASS_DT_SHADER
        };
        let simple_glass_source = if self.has_storage_buffers {
            SIMPLE_GLASS_SHADER
        } else {
            SIMPLE_GLASS_DT_SHADER
        };

        let glass_shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Glass Shader"),
                source: wgpu::ShaderSource::Wgsl(glass_source.into()),
            });

        let simple_glass_shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Simple Glass Shader"),
                source: wgpu::ShaderSource::Wgsl(simple_glass_source.into()),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Glass Pipeline Layout"),
                bind_group_layouts: &[&self.bind_group_layouts.glass],
                push_constant_ranges: &[],
            });

        let blend_state = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let color_targets = &[Some(wgpu::ColorTargetState {
            format: self.texture_format,
            blend: Some(blend_state),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        self.effect_pipelines.glass = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Glass Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &glass_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &glass_shader,
                    entry_point: Some("fs_main"),
                    targets: color_targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));

        self.effect_pipelines.simple_glass = Some(self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("Simple Glass Pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &simple_glass_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &simple_glass_shader,
                    entry_point: Some("fs_main"),
                    targets: color_targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        ));
    }

    /// Clear a texture view to a solid color
    pub fn clear_target(&mut self, target: &wgpu::TextureView, color: wgpu::Color) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Clear Target Encoder"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Target Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render images to a texture view
    ///
    /// # Arguments
    /// * `target` - The target texture view to render to
    /// * `image_view` - The image texture view to sample from
    /// * `instances` - The image instances to render
    pub fn render_images(
        &mut self,
        target: &wgpu::TextureView,
        image_view: &wgpu::TextureView,
        instances: &[GpuImageInstance],
    ) {
        if instances.is_empty() {
            return;
        }

        // Ensure pipeline is created
        self.ensure_image_pipeline();

        let image_pipeline = self.image_pipeline.as_ref().unwrap();

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update instance buffer
        self.queue.write_buffer(
            &image_pipeline.instance_buffer,
            0,
            bytemuck::cast_slice(instances),
        );

        // Create bind group for this image
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Image Bind Group"),
            layout: &image_pipeline.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.buffers.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(image_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&image_pipeline.sampler),
                },
            ],
        });

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Image Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Image Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Preserve existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&image_pipeline.pipeline);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.set_vertex_buffer(0, image_pipeline.instance_buffer.slice(..));
            if let Some((sx, sy, sw, sh)) = self.pending_scissor {
                render_pass.set_scissor_rect(sx, sy, sw, sh);
            }
            render_pass.draw(0..6, 0..instances.len() as u32);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Layer Texture Cache Accessors
    // ─────────────────────────────────────────────────────────────────────────

    /// Render dynamic RGBA images (video frames, camera preview, etc.)
    ///
    /// Uploads each image as a temporary GPU texture and renders it
    /// to the destination rect using the image pipeline.
    pub fn render_dynamic_images(
        &mut self,
        target: &wgpu::TextureView,
        images: &[crate::primitives::DynamicImage],
    ) {
        for img in images {
            if img.data.len() != (img.width * img.height * 4) as usize {
                continue; // Invalid RGBA data
            }

            // Create temporary GPU texture from RGBA data
            let gpu_image = crate::image::GpuImage::from_rgba(
                &self.device,
                &self.queue,
                &img.data,
                img.width,
                img.height,
                Some("dynamic_image"),
            );

            // Create an instance for the image pipeline
            let instance = GpuImageInstance::new(
                img.dest.x(),
                img.dest.y(),
                img.dest.width(),
                img.dest.height(),
            )
            .with_opacity(img.opacity)
            .with_border_radius(img.corner_radius);

            // Render using the existing image pipeline
            self.render_images(target, gpu_image.view(), &[instance]);
        }
    }

    // ─── Custom Render Pass API ────────────────────────────────────────────

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

    /// Get a reference to the layer texture cache
    pub fn layer_texture_cache(&self) -> &LayerTextureCache {
        &self.layer_texture_cache
    }

    /// Get a mutable reference to the layer texture cache
    pub fn layer_texture_cache_mut(&mut self) -> &mut LayerTextureCache {
        &mut self.layer_texture_cache
    }

    /// Acquire a layer texture from the cache
    ///
    /// If a matching texture exists in the pool, it will be reused.
    /// Otherwise, a new texture will be created.
    pub fn acquire_layer_texture(&mut self, size: (u32, u32), with_depth: bool) -> LayerTexture {
        self.layer_texture_cache
            .acquire(&self.device, size, with_depth)
    }

    /// Release a layer texture back to the cache pool
    pub fn release_layer_texture(&mut self, texture: LayerTexture) {
        self.layer_texture_cache.release(texture);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Layer Composition
    // ─────────────────────────────────────────────────────────────────────────────

    /// Create a bind group for layer composition
    fn create_layer_composite_bind_group(
        &self,
        uniform_buffer: &wgpu::Buffer,
        layer_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        self.create_layer_composite_bind_group_with_dest(
            uniform_buffer,
            layer_view,
            sampler,
            &self.dummy_blend_dest_view,
            sampler,
        )
    }

    fn create_layer_composite_bind_group_with_dest(
        &self,
        uniform_buffer: &wgpu::Buffer,
        layer_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        dest_view: &wgpu::TextureView,
        dest_sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Layer Composite Bind Group"),
            layout: &self.bind_group_layouts.layer_composite,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(layer_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(dest_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(dest_sampler),
                },
            ],
        })
    }

    /// Composite a layer texture onto a target
    ///
    /// Uses the LAYER_COMPOSITE_SHADER to blend the layer onto the target
    /// with the specified blend mode and opacity.
    #[allow(clippy::too_many_arguments)]
    pub fn composite_layer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        layer: &LayerTexture,
        dest_x: f32,
        dest_y: f32,
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) {
        // Create uniform buffer for this composition
        let uniforms = crate::primitives::LayerCompositeUniforms::new(
            layer.size,
            dest_x,
            dest_y,
            (self.viewport_size.0 as f32, self.viewport_size.1 as f32),
            opacity,
            blend_mode,
        );

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Layer Composite Uniforms"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        // Create sampler
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Layer Composite Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create bind group
        let bind_group =
            self.create_layer_composite_bind_group(&uniform_buffer, &layer.view, &sampler);

        // Create render pass and draw
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Layer Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, // Preserve existing content
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&self.pipelines.layer_composite);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..6, 0..1); // 6 vertices for quad (2 triangles)
    }

    /// Composite a layer with source/dest rectangle mapping
    ///
    /// Allows sampling a sub-region of the layer texture and placing it
    /// at a specific destination in the target.
    #[allow(clippy::too_many_arguments)]
    pub fn composite_layer_region(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        layer: &LayerTexture,
        source_rect: blinc_core::Rect,
        dest_rect: blinc_core::Rect,
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) {
        // Convert source rect to normalized UV coordinates
        let layer_w = layer.size.0 as f32;
        let layer_h = layer.size.1 as f32;
        let source_uv = [
            source_rect.x() / layer_w,
            source_rect.y() / layer_h,
            source_rect.width() / layer_w,
            source_rect.height() / layer_h,
        ];

        let uniforms = crate::primitives::LayerCompositeUniforms::with_source_rect(
            source_uv,
            [
                dest_rect.x(),
                dest_rect.y(),
                dest_rect.width(),
                dest_rect.height(),
            ],
            (self.viewport_size.0 as f32, self.viewport_size.1 as f32),
            opacity,
            blend_mode,
        );

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Layer Composite Uniforms"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Layer Composite Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group =
            self.create_layer_composite_bind_group(&uniform_buffer, &layer.view, &sampler);

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Layer Composite Pass"),
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

        render_pass.set_pipeline(&self.pipelines.layer_composite);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..6, 0..1);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Effect Application Methods
    // ─────────────────────────────────────────────────────────────────────────────

    /// Apply a single Kawase blur pass
    ///
    /// Renders from `input` to `output` using the blur shader with the specified
    /// radius and iteration index.
    ///
    /// `blur_alpha`: if true, blurs both RGB and alpha (for soft shadow edges);
    ///               if false, preserves alpha while blurring RGB (for element blur)
    /// Apply multi-pass Kawase blur, batched into a single GPU submission.
    ///
    /// Uses ping-pong rendering between two textures. All passes share one
    /// command encoder for minimal GPU synchronization overhead.
    ///
    /// `blur_alpha`: if true, blurs both RGB and alpha (for soft shadow edges);
    ///               if false, preserves alpha while blurring RGB (for element blur)
    ///
    /// Returns the final output texture (caller should release temp textures).
    pub fn apply_blur_with_alpha(
        &mut self,
        input: &LayerTexture,
        radius: f32,
        passes: u32,
        blur_alpha: bool,
    ) -> LayerTexture {
        self.ensure_blur_pipeline();

        if passes == 0 {
            // No blur needed, return a copy
            let output = self
                .layer_texture_cache
                .acquire(&self.device, input.size, false);
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Blur Copy Encoder"),
                });
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &input.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &output.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: input.size.0,
                    height: input.size.1,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(std::iter::once(encoder.finish()));
            return output;
        }

        let size = input.size;
        let blur_alpha_u32: u32 = if blur_alpha { 1 } else { 0 };

        // Write per-pass uniforms to pre-allocated buffer pool (no allocation)
        let blur_pool = self.buffers.blur_uniforms_pool.as_ref().unwrap();
        for i in 0..passes {
            self.queue.write_buffer(
                &blur_pool[i as usize],
                0,
                bytemuck::bytes_of(&BlurUniforms {
                    texel_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
                    radius,
                    iteration: i,
                    blur_alpha: blur_alpha_u32,
                    _pad1: 0.0,
                    _pad2: 0.0,
                    _pad3: 0.0,
                }),
            );
        }

        // For ping-pong we need two temp textures
        let temp_a = self.layer_texture_cache.acquire(&self.device, size, false);
        let temp_b = self.layer_texture_cache.acquire(&self.device, size, false);

        // Pre-create bind groups: pass 0 reads input, subsequent passes alternate temp_a/temp_b
        let bind_groups: Vec<wgpu::BindGroup> = (0..passes)
            .map(|i| {
                let input_view = if i == 0 {
                    &input.view
                } else if i % 2 == 1 {
                    &temp_a.view
                } else {
                    &temp_b.view
                };
                let blur_pool = self.buffers.blur_uniforms_pool.as_ref().unwrap();
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Blur Effect Bind Group"),
                    layout: &self.bind_group_layouts.blur,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: blur_pool[i as usize].as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(input_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                        },
                    ],
                })
            })
            .collect();

        // Single command encoder for all passes
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blur Multi-Pass Encoder"),
            });

        for i in 0..passes {
            let output_view = if i % 2 == 0 {
                &temp_a.view
            } else {
                &temp_b.view
            };

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blur Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(self.effect_pipelines.blur.as_ref().unwrap());
            render_pass.set_bind_group(0, &bind_groups[i as usize], &[]);
            render_pass.draw(0..6, 0..1);
        }

        // Single GPU submission for all blur passes
        self.queue.submit(std::iter::once(encoder.finish()));

        // Determine which texture has the final blurred result
        let (result, unused) = if passes % 2 == 1 {
            (temp_a, temp_b)
        } else {
            (temp_b, temp_a)
        };
        self.layer_texture_cache.release(unused);

        result
    }

    /// Apply multi-pass Kawase blur (CSS filter blur)
    ///
    /// Blurs both RGB and alpha channels, producing soft edges.
    pub fn apply_blur(&mut self, input: &LayerTexture, radius: f32, passes: u32) -> LayerTexture {
        self.apply_blur_with_alpha(input, radius, passes, false)
    }

    /// Apply multi-pass Kawase blur (shadow blur - blurs alpha for soft edges)
    ///
    /// Used for drop shadow and glow effects where we need soft alpha falloff.
    pub fn apply_shadow_blur(
        &mut self,
        input: &LayerTexture,
        radius: f32,
        passes: u32,
    ) -> LayerTexture {
        self.apply_blur_with_alpha(input, radius, passes, true)
    }

    /// Apply color matrix transformation
    ///
    /// Transforms colors using a 4x5 matrix (4x4 matrix + offset column).
    /// Useful for grayscale, sepia, saturation, brightness, contrast, etc.
    pub fn apply_color_matrix(
        &mut self,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        matrix: &[f32; 20],
    ) {
        self.ensure_color_matrix_pipeline();

        let uniforms = ColorMatrixUniforms::from_matrix(matrix);

        // Use cached buffer instead of creating per-pass
        let cm_buf = self.buffers.color_matrix_uniforms.as_ref().unwrap();
        self.queue
            .write_buffer(cm_buf, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Color Matrix Effect Bind Group"),
            layout: &self.bind_group_layouts.color_matrix,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: cm_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Color Matrix Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Color Matrix Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(self.effect_pipelines.color_matrix.as_ref().unwrap());
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Apply drop shadow effect
    ///
    /// Takes a pre-blurred texture (for shadow shape) and the original texture (for compositing).
    /// The blurred texture's alpha is used to create the shadow, which is then colored and
    /// composited behind the original content.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_drop_shadow(
        &mut self,
        blurred_input: &wgpu::TextureView,
        original_input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        size: (u32, u32),
        offset: (f32, f32),
        blur_radius: f32,
        spread: f32,
        color: [f32; 4],
    ) {
        self.ensure_drop_shadow_pipeline();

        let uniforms = DropShadowUniforms {
            offset: [offset.0, offset.1],
            blur_radius,
            spread,
            color,
            texel_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
            _pad: [0.0, 0.0],
        };

        // Use cached buffer instead of creating per-pass
        let ds_buf = self.buffers.drop_shadow_uniforms.as_ref().unwrap();
        self.queue
            .write_buffer(ds_buf, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Drop Shadow Effect Bind Group"),
            layout: &self.bind_group_layouts.drop_shadow,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ds_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(blurred_input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(original_input),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Drop Shadow Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Drop Shadow Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(self.effect_pipelines.drop_shadow.as_ref().unwrap());
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Apply glow effect to a texture
    ///
    /// Creates a radial glow around the shape by finding distance to nearest opaque pixels
    /// and applying a smooth falloff based on blur and range parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_glow(
        &mut self,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        size: (u32, u32),
        color: [f32; 4],
        blur: f32,
        range: f32,
        opacity: f32,
    ) {
        self.ensure_glow_pipeline();

        let uniforms = GlowUniforms {
            color,
            blur,
            range,
            opacity,
            _pad0: 0.0,
            texel_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
            _pad1: [0.0, 0.0],
        };

        // Use cached buffer instead of creating per-pass
        let glow_buf = self.buffers.glow_uniforms.as_ref().unwrap();
        self.queue
            .write_buffer(glow_buf, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Glow Effect Bind Group"),
            layout: &self.bind_group_layouts.glow,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: glow_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Glow Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Glow Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(self.effect_pipelines.glow.as_ref().unwrap());
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Helper to create common color matrices
    pub fn grayscale_matrix() -> [f32; 20] {
        // Luminance weights (ITU-R BT.709)
        let r = 0.2126;
        let g = 0.7152;
        let b = 0.0722;
        [
            r, g, b, 0.0, 0.0, r, g, b, 0.0, 0.0, r, g, b, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }

    /// Create sepia tone color matrix
    pub fn sepia_matrix() -> [f32; 20] {
        [
            0.393, 0.769, 0.189, 0.0, 0.0, 0.349, 0.686, 0.168, 0.0, 0.0, 0.272, 0.534, 0.131, 0.0,
            0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }

    /// Create saturation adjustment matrix
    pub fn saturation_matrix(saturation: f32) -> [f32; 20] {
        let s = saturation;
        let r = 0.2126;
        let g = 0.7152;
        let b = 0.0722;
        let sr = (1.0 - s) * r;
        let sg = (1.0 - s) * g;
        let sb = (1.0 - s) * b;
        [
            sr + s,
            sg,
            sb,
            0.0,
            0.0,
            sr,
            sg + s,
            sb,
            0.0,
            0.0,
            sr,
            sg,
            sb + s,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
        ]
    }

    /// Create brightness adjustment matrix
    pub fn brightness_matrix(brightness: f32) -> [f32; 20] {
        let b = brightness - 1.0; // 0 = no change, positive = brighter
        [
            1.0, 0.0, 0.0, 0.0, b, 0.0, 1.0, 0.0, 0.0, b, 0.0, 0.0, 1.0, 0.0, b, 0.0, 0.0, 0.0,
            1.0, 0.0,
        ]
    }

    /// Create contrast adjustment matrix
    pub fn contrast_matrix(contrast: f32) -> [f32; 20] {
        let c = contrast;
        let t = (1.0 - c) / 2.0;
        [
            c, 0.0, 0.0, 0.0, t, 0.0, c, 0.0, 0.0, t, 0.0, 0.0, c, 0.0, t, 0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }

    /// Create invert color matrix
    pub fn invert_matrix() -> [f32; 20] {
        [
            -1.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 1.0, 0.0,
        ]
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Layer Command Processing
    // ─────────────────────────────────────────────────────────────────────────────

    /// Calculate how much layer effects extend beyond the original content bounds.
    ///
    /// Returns (left, top, right, bottom) expansion in pixels.
    /// Blur expands bounds so the soft-edge falloff has room to render.
    fn calculate_effect_expansion(effects: &[blinc_core::LayerEffect]) -> (f32, f32, f32, f32) {
        use blinc_core::LayerEffect;

        let mut left = 0.0f32;
        let mut top = 0.0f32;
        let mut right = 0.0f32;
        let mut bottom = 0.0f32;

        for effect in effects {
            match effect {
                LayerEffect::Blur { radius, .. } => {
                    // Blur softens edges, which extends beyond original bounds.
                    // ~2x radius covers the visible falloff of Kawase blur.
                    let expand = radius * 2.0;
                    left = left.max(expand);
                    top = top.max(expand);
                    right = right.max(expand);
                    bottom = bottom.max(expand);
                }
                LayerEffect::DropShadow {
                    offset_x,
                    offset_y,
                    blur,
                    spread,
                    ..
                } => {
                    // Shadow expands based on blur, spread, and offset
                    let blur_expand = blur * 2.0; // 2x blur radius is enough
                    let spread_expand = spread.max(0.0);
                    let total_expand = blur_expand + spread_expand;

                    // Left/top expansion: when offset is negative, shadow goes that direction
                    left = left.max(total_expand + (-offset_x).max(0.0));
                    top = top.max(total_expand + (-offset_y).max(0.0));
                    // Right/bottom expansion: when offset is positive, shadow goes that direction
                    right = right.max(total_expand + offset_x.max(0.0));
                    bottom = bottom.max(total_expand + offset_y.max(0.0));
                }
                LayerEffect::Glow { blur, range, .. } => {
                    // Glow expands equally in all directions
                    let expand = (blur + range) * 2.0; // Account for range
                    left = left.max(expand);
                    top = top.max(expand);
                    right = right.max(expand);
                    bottom = bottom.max(expand);
                }
                LayerEffect::ColorMatrix { .. } | LayerEffect::MaskImage { .. } => {
                    // These don't expand bounds
                }
            }
        }

        (left, top, right, bottom)
    }

    /// No-op: mask images must be pre-loaded via `load_mask_image_rgba()`.
    fn load_mask_image(&mut self, _url: &str) {
        // Mask images are loaded externally (in blinc_app context) and cached
        // via load_mask_image_rgba() before the render pass begins.
    }

    /// Pre-load a mask image from RGBA pixel data.
    /// Call this before rendering to ensure mask textures are available.
    pub fn load_mask_image_rgba(&mut self, url: &str, pixels: &[u8], width: u32, height: u32) {
        if self.mask_image_cache.contains_key(url) {
            return;
        }
        // Mask images are loaded once and sampled every frame the
        // masked element is visible — a textbook case for BC. The
        // auto encoder picks BC1 when the mask is effectively
        // opaque (rare for masks but cheap to check) and BC3
        // otherwise to preserve the alpha channel the mask depends
        // on. Falls back to uncompressed upload on devices without
        // BC support or in builds without the `bc-encode` feature.
        //
        // The 256-px floor matches the 2D image cache's
        // `bc_eligible` heuristic: BC's 4×4 block quantization
        // puts visible banding into small alpha ramps, which is
        // exactly the signal a mask carries. Large masks
        // (full-viewport gradient overlays, photo-style alpha
        // cutouts) still compress.
        // Same alignment + size floor as the `bc_eligible` helper in
        // blinc_app: BC formats need multiple-of-4 dimensions (wgpu
        // validation), and sub-256 masks produce visible block
        // banding in alpha ramps.
        let bc_ok = self.has_texture_compression_bc
            && width % 4 == 0
            && height % 4 == 0
            && width >= 256
            && height >= 256;
        let label = format!("mask:{}", url);
        let gpu_img = crate::image::GpuImage::from_rgba_maybe_compressed(
            &self.device,
            &self.queue,
            pixels,
            width,
            height,
            false,
            bc_ok,
            Some(&label),
        );
        self.mask_image_cache.insert(url.to_string(), gpu_img);
    }

    /// Check if a mask image is already loaded in cache
    pub fn has_mask_image(&self, url: &str) -> bool {
        self.mask_image_cache.contains_key(url)
    }

    /// Apply mask image effect: multiplies element alpha by mask value
    fn apply_mask_image_effect(
        &mut self,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        image_url: &str,
        mask_mode: u32,
    ) {
        self.ensure_mask_image_pipeline();
        let mask_img = match self.mask_image_cache.get(image_url) {
            Some(img) => img,
            None => return,
        };

        let uniforms = MaskImageUniforms {
            mask_mode,
            _pad: [0.0; 3],
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Mask Image Uniforms"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Mask Image Effect Bind Group"),
            layout: &self.bind_group_layouts.mask_image,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(mask_img.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Mask Image Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Mask Image Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(self.effect_pipelines.mask_image.as_ref().unwrap());
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Apply layer effects to a texture
    ///
    /// Processes a list of LayerEffects in order and returns the final result.
    /// The input texture is not modified; a new texture with effects applied is returned.
    pub fn apply_layer_effects(
        &mut self,
        input: &LayerTexture,
        effects: &[blinc_core::LayerEffect],
    ) -> LayerTexture {
        use blinc_core::LayerEffect;

        if effects.is_empty() {
            // No effects, just return a copy
            let output = self
                .layer_texture_cache
                .acquire(&self.device, input.size, false);
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Layer Effect Copy Encoder"),
                });
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &input.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &output.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: input.size.0,
                    height: input.size.1,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(std::iter::once(encoder.finish()));
            return output;
        }

        let size = input.size;
        // Track ownership: effects that produce a new texture pass ownership here.
        // We avoid a redundant copy by using the input directly for the first effect
        // and only copying when a non-blur effect needs a mutable working texture.
        let mut current: Option<LayerTexture> = None;

        for effect in effects {
            // Get the current working texture or the original input
            let working = current.as_ref().unwrap_or(input);

            match effect {
                LayerEffect::Blur { radius, quality: _ } => {
                    // Blur reads from working and produces a new texture (no copy needed)
                    let passes = ((*radius / 2.0).ceil().max(2.0) as u32).min(8);
                    let blurred = self.apply_blur(working, *radius, passes);
                    if let Some(prev) = current.take() {
                        self.layer_texture_cache.release(prev);
                    }
                    current = Some(blurred);
                }

                LayerEffect::DropShadow {
                    offset_x,
                    offset_y,
                    blur,
                    spread,
                    color,
                } => {
                    let temp = self.layer_texture_cache.acquire(&self.device, size, false);
                    self.apply_drop_shadow(
                        &working.view,
                        &working.view,
                        &temp.view,
                        size,
                        (*offset_x, *offset_y),
                        *blur,
                        *spread,
                        [color.r, color.g, color.b, color.a],
                    );
                    if let Some(prev) = current.take() {
                        self.layer_texture_cache.release(prev);
                    }
                    current = Some(temp);
                }

                LayerEffect::Glow {
                    color,
                    blur,
                    range,
                    opacity,
                } => {
                    let temp = self.layer_texture_cache.acquire(&self.device, size, false);
                    self.apply_glow(
                        &working.view,
                        &temp.view,
                        size,
                        [color.r, color.g, color.b, color.a],
                        *blur,
                        *range,
                        *opacity,
                    );
                    if let Some(prev) = current.take() {
                        self.layer_texture_cache.release(prev);
                    }
                    current = Some(temp);
                }

                LayerEffect::ColorMatrix { matrix } => {
                    let temp = self.layer_texture_cache.acquire(&self.device, size, false);
                    self.apply_color_matrix(&working.view, &temp.view, matrix);
                    if let Some(prev) = current.take() {
                        self.layer_texture_cache.release(prev);
                    }
                    current = Some(temp);
                }

                LayerEffect::MaskImage {
                    image_url,
                    mask_mode,
                } => {
                    // Load mask image if not cached
                    self.load_mask_image(image_url);
                    // Apply mask if the texture was loaded successfully
                    if self.mask_image_cache.contains_key(image_url.as_str()) {
                        let temp = self.layer_texture_cache.acquire(&self.device, size, false);
                        let mode_val = match mask_mode {
                            blinc_core::MaskMode::Alpha => 0u32,
                            blinc_core::MaskMode::Luminance => 1u32,
                        };
                        self.apply_mask_image_effect(
                            &working.view,
                            &temp.view,
                            image_url,
                            mode_val,
                        );
                        if let Some(prev) = current.take() {
                            self.layer_texture_cache.release(prev);
                        }
                        current = Some(temp);
                    }
                }
            }
        }

        // If no effect produced a new texture (shouldn't happen since effects is non-empty),
        // fall back to a copy
        current.unwrap_or_else(|| {
            let output = self
                .layer_texture_cache
                .acquire(&self.device, input.size, false);
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Layer Effect Fallback Copy"),
                });
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &input.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &output.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: input.size.0,
                    height: input.size.1,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(std::iter::once(encoder.finish()));
            output
        })
    }

    /// Composite two textures together
    ///
    /// Blends `top` over `bottom` using the specified blend mode and opacity.
    pub fn composite_textures(
        &mut self,
        bottom: &wgpu::TextureView,
        top: &wgpu::TextureView,
        output: &wgpu::TextureView,
        size: (u32, u32),
        blend_mode: blinc_core::BlendMode,
        opacity: f32,
    ) {
        use crate::primitives::CompositeUniforms;

        let uniforms = CompositeUniforms {
            opacity,
            blend_mode: blend_mode as u32,
            _padding: [0.0; 2],
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Composite Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composite Bind Group"),
            layout: &self.bind_group_layouts.composite,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(bottom),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(top),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Composite Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Composite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.composite);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a range of primitives to a target
    fn render_primitive_range(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        start_idx: usize,
        end_idx: usize,
        clear_color: [f64; 4],
    ) {
        if start_idx >= end_idx {
            return;
        }

        // Extract the primitive range
        let _primitive_count = end_idx - start_idx;
        let primitives = &batch.primitives[start_idx..end_idx];

        if primitives.is_empty() {
            return;
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Sort and upload primitive range
        let sdf_ranges = self.upload_sorted_primitives(primitives);

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Primitive Range Render Encoder"),
            });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Primitive Range Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0],
                            g: clear_color[1],
                            b: clear_color[2],
                            a: clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            Self::draw_split_sdf(
                &mut render_pass,
                &self.pipelines,
                &sdf_ranges,
                false,
                self.sdf_vb_buffer(),
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a range of primitives to a tight-fit texture with offset
    ///
    /// This method renders primitives to a texture sized to fit the content,
    /// offsetting primitive positions so they start at (0,0).
    ///
    /// Returns the texture AND the actual content size (which may differ from
    /// texture.size due to pool bucket rounding).
    #[allow(clippy::too_many_arguments)]
    fn render_primitive_range_tight(
        &mut self,
        batch: &PrimitiveBatch,
        start_idx: usize,
        end_idx: usize,
        layer_pos: (f32, f32),
        layer_size: (f32, f32),
        effect_expansion: (f32, f32, f32, f32), // (left, top, right, bottom)
    ) -> (LayerTexture, (u32, u32)) {
        // Forward to the path-aware variant with an empty path range.
        self.render_layer_range_tight(
            batch,
            start_idx,
            end_idx,
            0,
            0,
            0,
            0,
            layer_pos,
            layer_size,
            effect_expansion,
        )
    }

    /// Path-aware variant of `render_primitive_range_tight` — also
    /// renders a slice of `batch.paths` (vertices `[pv_start..pv_end)`,
    /// indices `[pi_start..pi_end)`) into the tight texture, with
    /// vertex positions offset to the layer-local origin. Lottie
    /// shapes go through the path pipeline (lyon-tessellated
    /// triangles), so a layer that contains only paths needs this
    /// variant — the primitive-only one would render an empty
    /// offscreen even though the paths exist in the batch.
    #[allow(clippy::too_many_arguments)]
    fn render_layer_range_tight(
        &mut self,
        batch: &PrimitiveBatch,
        start_idx: usize,
        end_idx: usize,
        pv_start: usize,
        pv_end: usize,
        pi_start: usize,
        pi_end: usize,
        layer_pos: (f32, f32),
        layer_size: (f32, f32),
        effect_expansion: (f32, f32, f32, f32),
    ) -> (LayerTexture, (u32, u32)) {
        // Calculate tight texture size including effect expansion
        let texture_width = (layer_size.0 + effect_expansion.0 + effect_expansion.2)
            .ceil()
            .max(1.0) as u32;
        let texture_height = (layer_size.1 + effect_expansion.1 + effect_expansion.3)
            .ceil()
            .max(1.0) as u32;

        // Round up to reasonable sizes for cache efficiency (64px increments)
        let texture_width = (texture_width.div_ceil(64) * 64).min(self.viewport_size.0);
        let texture_height = (texture_height.div_ceil(64) * 64).min(self.viewport_size.1);

        // This is the actual content size (64px rounded), which may differ from
        // the texture returned by acquire() due to bucket rounding
        let content_size = (texture_width, texture_height);

        // Acquire a texture of at least the tight size
        let layer_texture = self
            .layer_texture_cache
            .acquire(&self.device, content_size, false);

        let primitives = if start_idx < end_idx {
            &batch.primitives[start_idx..end_idx]
        } else {
            &[][..]
        };
        let path_verts = if pv_end > pv_start && pv_end <= batch.paths.vertices.len() {
            &batch.paths.vertices[pv_start..pv_end]
        } else {
            &[][..]
        };
        let path_indices = if pi_end > pi_start && pi_end <= batch.paths.indices.len() {
            &batch.paths.indices[pi_start..pi_end]
        } else {
            &[][..]
        };

        if primitives.is_empty() && path_verts.is_empty() {
            return (layer_texture, content_size);
        }

        // Offset primitives so content starts at (effect_expansion.left, effect_expansion.top)
        // This leaves room for effects on the left/top edges
        let offset_x = layer_pos.0 - effect_expansion.0;
        let offset_y = layer_pos.1 - effect_expansion.1;

        let offset_primitives: Vec<GpuPrimitive> = primitives
            .iter()
            .map(|p| {
                let mut op = *p;
                op.bounds[0] -= offset_x;
                op.bounds[1] -= offset_y;
                // Also offset clip bounds if they're valid (not the "no clip" default)
                let has_real_clip = op.clip_bounds[0] > -5000.0 && op.clip_bounds[2] < 90000.0;
                if has_real_clip {
                    op.clip_bounds[0] -= offset_x;
                    op.clip_bounds[1] -= offset_y;
                }
                op
            })
            .collect();

        // Mesh primitives (`type_info[0] == 9`) read their triangle
        // vertex positions from `aux_data[aux_offset..aux_offset + 2]`,
        // not from `bounds`. Those positions are screen-space at push
        // time, so the `bounds` offset above doesn't translate them
        // into the tight texture's coordinate frame — vertices land
        // outside the texture viewport and the rasteriser clips the
        // triangle, leaving the tight texture empty. Build a translated
        // copy of `aux_data` for this pass: clone the original, then
        // subtract the offset from the position vec4s of every mesh
        // primitive in the layer range. The optional per-vertex colour
        // entries that follow (when `type_info[3] == 1`) stay unchanged.
        let mut tight_aux_data: Vec<[f32; 4]> = batch.aux_data.clone();
        let mut needs_aux_upload = false;
        for op in &offset_primitives {
            if op.type_info[0] != 9 {
                continue;
            }
            let aux_off = op.border[2] as usize;
            if aux_off + 1 >= tight_aux_data.len() {
                continue;
            }
            tight_aux_data[aux_off][0] -= offset_x;
            tight_aux_data[aux_off][1] -= offset_y;
            tight_aux_data[aux_off][2] -= offset_x;
            tight_aux_data[aux_off][3] -= offset_y;
            tight_aux_data[aux_off + 1][0] -= offset_x;
            tight_aux_data[aux_off + 1][1] -= offset_y;
            // pack1.zw is unused (padding) — leave alone.
            needs_aux_upload = true;
        }

        // Build the offset PathVertex slice + rebased index buffer.
        // Indices in the source batch reference vertices in
        // `batch.paths.vertices` directly; after slicing, the local
        // vertex array starts at 0, so each index needs `pv_start`
        // subtracted to point at the right vertex inside the slice.
        let offset_path_vertices: Vec<crate::path::PathVertex> = path_verts
            .iter()
            .map(|v| {
                let mut nv = *v;
                nv.position[0] -= offset_x;
                nv.position[1] -= offset_y;
                let has_real_clip = nv.clip_bounds[0] > -5000.0 && nv.clip_bounds[2] < 90000.0;
                if has_real_clip {
                    nv.clip_bounds[0] -= offset_x;
                    nv.clip_bounds[1] -= offset_y;
                }
                nv
            })
            .collect();
        let offset_path_indices: Vec<u32> = path_indices
            .iter()
            .map(|&i| i.saturating_sub(pv_start as u32))
            .collect();

        // Update uniforms with content size (the viewport for this tight render)
        let uniforms = Uniforms {
            viewport_size: [content_size.0 as f32, content_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Sort and upload offset primitives
        let sdf_ranges = self.upload_sorted_primitives(&offset_primitives);
        drop(offset_primitives);

        // Upload the offset-translated `aux_data` if any mesh
        // primitive needed translation. Same `self.buffers.aux_data`
        // the main pass uses — we restore it after `queue.submit`
        // so subsequent passes see the original screen-space data.
        // The buffer is already sized for `batch.aux_data`'s length
        // (the main pass uploaded the same vec earlier), so the
        // write fits without resizing or rebinding.
        if needs_aux_upload {
            if self.has_storage_buffers {
                self.queue.write_buffer(
                    &self.buffers.aux_data,
                    0,
                    bytemuck::cast_slice(&tight_aux_data),
                );
            } else {
                self.update_aux_data_texture(&tight_aux_data);
            }
        }

        // Upload offset path geometry to a transient buffer pair so
        // the shared `path_vertices` / `path_indices` buffers used by
        // the main pass aren't clobbered.
        use wgpu::util::DeviceExt;
        let path_vb = (!offset_path_vertices.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Tight Path VB"),
                    contents: bytemuck::cast_slice(&offset_path_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });
        let path_ib = (!offset_path_indices.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Tight Path IB"),
                    contents: bytemuck::cast_slice(&offset_path_indices),
                    usage: wgpu::BufferUsages::INDEX,
                })
        });
        let path_index_count = offset_path_indices.len() as u32;
        drop(offset_path_vertices);
        drop(offset_path_indices);

        // Path uniforms: full opacity (the blit applies the layer's
        // opacity), no clip, standard fill. CRITICAL — the path
        // shader reads `uniforms.viewport_size` to convert the
        // vertex position into NDC. The SDF-path `self.buffers.uniforms`
        // we just wrote is a DIFFERENT binding; paths see
        // `self.buffers.path_uniforms`. If we left it at the
        // Default (`[800, 600]`) while rendering into a, say,
        // 192×192 tight texture, the path would end up NDC-scaled
        // by the wrong viewport and land off-centre at massive
        // scale (visible as an oversized, drifted glass body).
        let path_uniforms = crate::primitives::PathUniforms {
            viewport_size: [content_size.0 as f32, content_size.1 as f32],
            ..crate::primitives::PathUniforms::default()
        };
        self.queue.write_buffer(
            &self.buffers.path_uniforms,
            0,
            bytemuck::bytes_of(&path_uniforms),
        );

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tight Render Encoder"),
            });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tight Primitive Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &layer_texture.view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if !primitives.is_empty() {
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                Self::draw_split_sdf(
                    &mut render_pass,
                    &self.pipelines,
                    &sdf_ranges,
                    false,
                    self.sdf_vb_buffer(),
                );
            }

            if let (Some(vb), Some(ib)) = (&path_vb, &path_ib) {
                render_pass.set_pipeline(&self.pipelines.path);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..path_index_count, 0, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Restore viewport uniforms for subsequent operations
        let restore_uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue.write_buffer(
            &self.buffers.uniforms,
            0,
            bytemuck::bytes_of(&restore_uniforms),
        );

        // Restore the screen-space `aux_data` so subsequent passes
        // (next layer's tight render, post-effect overlays, etc.)
        // see the original mesh vertex positions instead of the
        // tight-translated copy we wrote above.
        if needs_aux_upload {
            if self.has_storage_buffers {
                self.queue.write_buffer(
                    &self.buffers.aux_data,
                    0,
                    bytemuck::cast_slice(&batch.aux_data),
                );
            } else {
                self.update_aux_data_texture(&batch.aux_data);
            }
        }

        (layer_texture, content_size)
    }

    /// Rasterize an entire `PrimitiveBatch` (from a walker scratch
    /// pass) into a fresh tight `LayerTexture`. Used by the
    /// composited-layer path for CSS-animated subtrees: the walker
    /// peels the subtree's primitives into a per-node scratch batch
    /// (`GpuPaintContext::push_composite_layer` /
    /// `take_composite_layer_batches`), then this helper turns each
    /// scratch batch into a cached texture that
    /// `blit_tight_texture_to_target` composites per frame with the
    /// active animation transform applied.
    ///
    /// `layer_pos` is the physical-pixel screen position the
    /// primitives were emitted at (typically the screen-space AABB's
    /// top-left from `bg_primitive_aabb`). Primitives get translated
    /// by `-layer_pos` so they land at (0, 0) inside the texture.
    /// `layer_size` is the texture's logical content size; the
    /// actual texture may be larger due to `LayerTextureCache`'s
    /// bucket rounding. Both values are returned as
    /// `content_size`.
    ///
    /// Effect-expansion is zero — composite-promoted subtrees can't
    /// carry layer effects (the promotion predicate disqualifies
    /// `filter_blur`, `backdrop_*`, etc., and the underlying
    /// `LayerCommand` Push/Pop pairs aren't in the scratch batch).
    pub fn render_subtree_to_layer_texture(
        &mut self,
        batch: &PrimitiveBatch,
        layer_pos: (f32, f32),
        layer_size: (f32, f32),
    ) -> (LayerTexture, (u32, u32)) {
        self.render_layer_range_tight(
            batch,
            0,
            batch.primitives.len(),
            0,
            0,
            0,
            0,
            layer_pos,
            layer_size,
            (0.0, 0.0, 0.0, 0.0),
        )
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
            _pad: [0.0; 2],
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
            if let Some(target_ptr) = self.blend_target_ptr {
                let target_texture = unsafe { &*target_ptr };
                let temp =
                    self.layer_texture_cache
                        .acquire(&self.device, self.viewport_size, false);

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

    /// Override viewport size for offscreen rendering to a smaller texture.
    /// This swaps `self.viewport_size` so all render functions (text, images, SDF)
    /// use the offscreen size for NDC conversion. Must call `restore_viewport()` after.
    pub fn set_viewport_override(&mut self, size: (u32, u32)) {
        self.saved_viewport_size = Some(self.viewport_size);
        self.viewport_size = size;
    }

    /// Restore viewport size after offscreen rendering.
    pub fn restore_viewport(&mut self) {
        if let Some(saved) = self.saved_viewport_size.take() {
            self.viewport_size = saved;
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
            _pad: [0.0; 2],
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
            _pad: [0.0; 2],
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
