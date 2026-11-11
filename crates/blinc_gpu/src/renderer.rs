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
mod accessors;
mod bind_group_layouts;
mod blit;
mod buffers;
mod config;
mod construction;
mod device_setup;
mod effect_pipelines;
mod effects;
mod flow;
mod frame;
mod glass_frame;
mod images;
mod init;
mod layer_cache;
mod layers;
mod overlay_msaa;
mod overlay_render;
mod pipelines;
mod sdf_dispatch;
mod static_layer;
mod text;
mod viewports;
pub use config::{GpuMemoryBudget, RendererConfig, RendererError};
use config::{apply_renderer_config_overrides, env_u64, env_usize, log_renderer_config};
use device_setup::{
    PIPELINE_CACHE_HEADER_LEN, PIPELINE_CACHE_MAGIC, PIPELINE_CACHE_VERSION,
    device_required_limits, load_pipeline_cache_data, pipeline_cache_driver_key,
    requested_device_features, save_pipeline_cache_data, stable_hash_bytes, stable_hash_str,
};
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

/// Axis-aligned bounding box intersection test. Each rect is
/// `[x, y, w, h]` (top-left origin, screen pixels). Returns true when
/// the rects overlap (zero-area overlap counts as no intersection).
fn aabb_intersects(a: [f32; 4], b: [f32; 4]) -> bool {
    let [ax, ay, aw, ah] = a;
    let [bx, by, bw, bh] = b;
    ax + aw > bx && bx + bw > ax && ay + ah > by && by + bh > ay
}

// ─────────────────────────────────────────────────────────────────────────────
// GPU Memory Budget & Eviction
// ─────────────────────────────────────────────────────────────────────────────

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
