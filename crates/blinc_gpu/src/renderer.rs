//! GPU renderer implementation
//!
//! The main renderer that manages wgpu resources and executes render passes
//! for SDF primitives, glass effects, and text.

use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::gradient_texture::{GradientTextureCache, RasterizedGradient};
use crate::image::GpuImageInstance;
use crate::path::PathVertex;
use crate::primitives::{
    GlassUniforms, GpuGlassPrimitive, GpuGlyph, GpuPrimitive, PathUniforms, PrimitiveBatch,
    Uniforms,
};
use crate::shaders::{
    COMPOSITE_SHADER, GLASS_SHADER, IMAGE_SHADER, PATH_SHADER, SDF_SHADER, TEXT_SHADER,
};

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
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            // Reduced defaults for lower memory footprint (~1 MB total vs ~5+ MB)
            // These can still handle typical UI scenes while using less memory
            max_primitives: 2_000,     // ~384 KB (was 1.92 MB)
            max_glass_primitives: 100, // ~25 KB (was 256 KB)
            max_glyphs: 10_000,        // ~640 KB (was 3.2 MB)
            sample_count: 1,
            texture_format: None,
            unified_text_rendering: true, // Enabled for consistent transforms during animations
        }
    }
}

/// Render pipelines for different primitive types
struct Pipelines {
    /// Pipeline for SDF primitives (rects, circles, etc.)
    sdf: wgpu::RenderPipeline,
    /// Pipeline for SDF primitives rendering on top of existing content (1x sampled)
    sdf_overlay: wgpu::RenderPipeline,
    /// Pipeline for glass/vibrancy effects
    glass: wgpu::RenderPipeline,
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
}

/// Cached MSAA pipelines for dynamic sample counts
struct MsaaPipelines {
    /// SDF pipeline for this sample count
    sdf: wgpu::RenderPipeline,
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

/// Cached SDF bind group with glyph atlas textures (for unified text rendering)
struct CachedSdfWithGlyphs {
    /// Cached bind group with actual glyph atlas textures
    bind_group: wgpu::BindGroup,
    /// Pointer to grayscale atlas view when bind group was created (for invalidation)
    atlas_view_ptr: *const wgpu::TextureView,
    /// Pointer to color atlas view when bind group was created (for invalidation)
    color_atlas_view_ptr: *const wgpu::TextureView,
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
    device: Arc<wgpu::Device>,
    /// Command queue
    queue: Arc<wgpu::Queue>,
    /// Render pipelines
    pipelines: Pipelines,
    /// Cached MSAA pipelines for overlay rendering
    msaa_pipelines: Option<MsaaPipelines>,
    /// GPU buffers
    buffers: Buffers,
    /// Bind groups
    bind_groups: BindGroups,
    /// Bind group layouts
    bind_group_layouts: BindGroupLayouts,
    /// Current viewport size
    viewport_size: (u32, u32),
    /// Renderer configuration
    config: RendererConfig,
    /// Current frame time (for animations)
    time: f32,
    /// Resolved texture format used by pipelines
    texture_format: wgpu::TextureFormat,
    /// Lazily-created image pipeline and resources
    image_pipeline: Option<ImagePipeline>,
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
    /// Cached SDF bind group with actual glyph atlas textures (for unified text rendering)
    cached_sdf_with_glyphs: Option<CachedSdfWithGlyphs>,
    /// Gradient texture cache for multi-stop gradient support on paths
    gradient_texture_cache: GradientTextureCache,
    /// Placeholder image texture (1x1 white) for path bind group when no image is used
    placeholder_path_image_view: wgpu::TextureView,
    /// Sampler for path image textures
    path_image_sampler: wgpu::Sampler,
}

/// Image rendering pipeline (created lazily on first image render)
struct ImagePipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    instance_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
}

struct BindGroupLayouts {
    sdf: wgpu::BindGroupLayout,
    glass: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    text: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    composite: wgpu::BindGroupLayout,
    path: wgpu::BindGroupLayout,
}

impl GpuRenderer {
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

    /// Create a new renderer without a surface (for headless rendering)
    pub async fn new(config: RendererConfig) -> Result<Self, RendererError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
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
            .ok_or(RendererError::AdapterNotFound)?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Blinc GPU Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    // MemoryUsage hint tells the driver to prefer lower memory over performance.
                    // This helps reduce RSS on integrated GPUs (Apple Silicon) where GPU memory
                    // is shared with CPU and counts against process memory.
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
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
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
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
            .ok_or(RendererError::AdapterNotFound)?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Blinc GPU Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    // MemoryUsage hint tells the driver to prefer lower memory over performance.
                    // This helps reduce RSS on integrated GPUs (Apple Silicon) where GPU memory
                    // is shared with CPU and counts against process memory.
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
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
                surface_caps
                    .formats
                    .iter()
                    .find(|f| f.is_srgb())
                    .copied()
                    .unwrap_or(surface_caps.formats[0])
            }
        });
        tracing::debug!("Selected texture format: {:?}", texture_format);

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

    fn create_renderer(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        texture_format: wgpu::TextureFormat,
        config: RendererConfig,
        viewport_size: (u32, u32),
    ) -> Result<Self, RendererError> {
        // Create bind group layouts
        let bind_group_layouts = Self::create_bind_group_layouts(&device);

        // Create shaders
        let sdf_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Shader"),
            source: wgpu::ShaderSource::Wgsl(SDF_SHADER.into()),
        });

        let glass_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Glass Shader"),
            source: wgpu::ShaderSource::Wgsl(GLASS_SHADER.into()),
        });

        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text Shader"),
            source: wgpu::ShaderSource::Wgsl(TEXT_SHADER.into()),
        });

        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
        });

        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Path Shader"),
            source: wgpu::ShaderSource::Wgsl(PATH_SHADER.into()),
        });

        // Create pipelines
        let pipelines = Self::create_pipelines(
            &device,
            &bind_group_layouts,
            &sdf_shader,
            &glass_shader,
            &text_shader,
            &composite_shader,
            &path_shader,
            texture_format,
            config.sample_count,
        );

        // Create buffers
        let buffers = Self::create_buffers(&device, &config);

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
            wgpu::ImageCopyTexture {
                texture: &placeholder_path_image,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255u8, 255, 255, 255], // White pixel
            wgpu::ImageDataLayout {
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
        );

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            pipelines,
            msaa_pipelines: None,
            buffers,
            bind_groups,
            bind_group_layouts,
            viewport_size,
            config,
            time: 0.0,
            texture_format,
            image_pipeline: None,
            cached_msaa: None,
            cached_glass: None,
            cached_text: None,
            placeholder_glyph_atlas_view,
            placeholder_color_glyph_atlas_view,
            glyph_sampler,
            cached_sdf_with_glyphs: None,
            gradient_texture_cache,
            placeholder_path_image_view,
            path_image_sampler,
        })
    }

    fn create_bind_group_layouts(device: &wgpu::Device) -> BindGroupLayouts {
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
                // Primitives storage buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
                // Glass primitives storage buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
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
                // Glyphs storage buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
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
            ],
        });

        BindGroupLayouts {
            sdf,
            glass,
            text,
            composite,
            path,
        }
    }

    fn create_pipelines(
        device: &wgpu::Device,
        layouts: &BindGroupLayouts,
        sdf_shader: &wgpu::ShaderModule,
        glass_shader: &wgpu::ShaderModule,
        text_shader: &wgpu::ShaderModule,
        composite_shader: &wgpu::ShaderModule,
        path_shader: &wgpu::ShaderModule,
        texture_format: wgpu::TextureFormat,
        sample_count: u32,
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

        let sdf = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SDF Pipeline"),
            layout: Some(&sdf_layout),
            vertex: wgpu::VertexState {
                module: sdf_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
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
                buffers: &[],
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

        // Glass pipeline - always uses sample_count=1 since it renders on resolved textures
        // (glass effects require sampling from a single-sampled backdrop texture)
        let glass_multisample_state = wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        let glass_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Glass Pipeline Layout"),
            bind_group_layouts: &[&layouts.glass],
            push_constant_ranges: &[],
        });

        let glass = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Glass Pipeline"),
            layout: Some(&glass_layout),
            vertex: wgpu::VertexState {
                module: glass_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: glass_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: glass_multisample_state,
            multiview: None,
            cache: None,
        });

        // Text pipeline
        let text_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Text Pipeline Layout"),
            bind_group_layouts: &[&layouts.text],
            push_constant_ranges: &[],
        });

        let text = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text Pipeline"),
            layout: Some(&text_layout),
            vertex: wgpu::VertexState {
                module: text_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
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
                buffers: &[],
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
        //   _padding: [u32; 2]       - 8 bytes, offset 72
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
            ],
        };

        let path = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Path Pipeline"),
            layout: Some(&path_layout),
            vertex: wgpu::VertexState {
                module: path_shader,
                entry_point: Some("vs_main"),
                buffers: &[path_vertex_layout.clone()],
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

        Pipelines {
            sdf,
            sdf_overlay,
            glass,
            text,
            text_overlay,
            composite,
            composite_overlay,
            path,
            path_overlay,
        }
    }

    fn create_buffers(device: &wgpu::Device, config: &RendererConfig) -> Buffers {
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniforms Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

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

        Buffers {
            uniforms,
            primitives,
            glass_primitives,
            glass_uniforms,
            glyphs,
            path_uniforms,
            path_vertices: None,
            path_indices: None,
        }
    }

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
    ) -> BindGroups {
        let sdf = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF Bind Group"),
            layout: &layouts.sdf,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffers.primitives.as_entire_binding(),
                },
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

        // Create SDF shader
        let sdf_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Shader (MSAA)"),
            source: wgpu::ShaderSource::Wgsl(SDF_SHADER.into()),
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

        // PathVertex layout
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
            path,
            sample_count,
        }
    }

    /// Resize the viewport
    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport_size = (width, height);
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
        self.device.poll(wgpu::Maintain::Wait);
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
        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update primitives buffer
        if !batch.primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.primitives),
            );
        }

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

            // Render SDF primitives
            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(&self.pipelines.sdf);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                // 6 vertices per quad (2 triangles), one instance per primitive
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
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

    /// Update path vertex and index buffers
    fn update_path_buffers(&mut self, batch: &PrimitiveBatch) {
        // Upload gradient texture if needed for multi-stop gradients
        if batch.paths.use_gradient_texture {
            if let Some(ref stops) = batch.paths.gradient_stops {
                let rasterized =
                    RasterizedGradient::from_stops(stops, crate::gradient_texture::SpreadMode::Pad);
                self.gradient_texture_cache.upload(&self.queue, &rasterized);
            }
        }

        // Update path uniforms with clip data and brush metadata from batch
        let path_uniforms = PathUniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            clip_bounds: batch.paths.clip_bounds,
            clip_radius: batch.paths.clip_radius,
            clip_type: batch.paths.clip_type,
            use_gradient_texture: if batch.paths.use_gradient_texture { 1 } else { 0 },
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

        // Update primitives buffer
        if !batch.primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.primitives),
            );
        }

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

            // Render SDF primitives
            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(&self.pipelines.sdf);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
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
    pub fn render_glass(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        batch: &PrimitiveBatch,
    ) {
        if batch.glass_primitives.is_empty() {
            return;
        }

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

        // Update glass primitives buffer
        self.queue.write_buffer(
            &self.buffers.glass_primitives,
            0,
            bytemuck::cast_slice(&batch.glass_primitives),
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
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Keep existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.glass);
            render_pass.set_bind_group(0, glass_bind_group, &[]);
            render_pass.draw(0..6, 0..batch.glass_primitives.len() as u32);
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
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
        backdrop_size: (u32, u32),
        batch: &PrimitiveBatch,
    ) {
        // Update uniforms for backdrop (half resolution)
        let backdrop_uniforms = Uniforms {
            viewport_size: [backdrop_size.0 as f32, backdrop_size.1 as f32],
            _padding: [0.0; 2],
        };

        // Update uniforms for main rendering
        let main_uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };

        // Update primitives buffer
        if !batch.primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.primitives),
            );
        }

        // Update glass primitives buffer
        if !batch.glass_primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.glass_primitives,
                0,
                bytemuck::cast_slice(&batch.glass_primitives),
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
        {
            self.queue.write_buffer(
                &self.buffers.uniforms,
                0,
                bytemuck::bytes_of(&backdrop_uniforms),
            );

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Backdrop Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: backdrop,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(&self.pipelines.sdf);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
            }
        }

        // Pass 2: Render background primitives to target (at full resolution)
        {
            self.queue.write_buffer(
                &self.buffers.uniforms,
                0,
                bytemuck::bytes_of(&main_uniforms),
            );

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Target Background Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(&self.pipelines.sdf);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
            }
        }

        // Pass 3: Render glass primitives with backdrop blur
        if !batch.glass_primitives.is_empty() {
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
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.glass);
            render_pass.set_bind_group(0, glass_bind_group, &[]);
            render_pass.draw(0..6, 0..batch.glass_primitives.len() as u32);
        }

        // Submit background and glass passes first
        self.queue.submit(std::iter::once(encoder.finish()));

        // Pass 4: Render foreground primitives (on top of glass)
        // This requires a separate submission because we need to overwrite the primitives buffer
        if !batch.foreground_primitives.is_empty() {
            // Upload foreground primitives to the buffer
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.foreground_primitives),
            );

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
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.sdf);
            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            render_pass.draw(0..6, 0..batch.foreground_primitives.len() as u32);

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
        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update primitives buffer
        if !batch.primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.primitives),
            );
        }

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

            // Render SDF primitives using overlay pipeline
            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(&self.pipelines.sdf_overlay);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a slice of primitives as overlay (LoadOp::Load, keeps existing content)
    ///
    /// This is used for interleaved z-layer rendering where primitives need
    /// to be rendered per-layer to properly interleave with text.
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

        // Update primitives buffer
        self.queue.write_buffer(
            &self.buffers.primitives,
            0,
            bytemuck::cast_slice(primitives),
        );

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
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render SDF primitives
            render_pass.set_pipeline(&self.pipelines.sdf_overlay);
            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            render_pass.draw(0..6, 0..primitives.len() as u32);
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
    ///
    /// # Arguments
    /// * `target` - The texture view to render to
    /// * `primitives` - The SDF primitives including text glyph primitives
    /// * `atlas_view` - The grayscale glyph atlas texture view
    /// * `color_atlas_view` - The color (RGBA) glyph atlas texture view for emoji
    pub fn render_primitives_overlay_with_glyphs(
        &mut self,
        target: &wgpu::TextureView,
        primitives: &[GpuPrimitive],
        atlas_view: &wgpu::TextureView,
        color_atlas_view: &wgpu::TextureView,
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

        // Update primitives buffer
        self.queue.write_buffer(
            &self.buffers.primitives,
            0,
            bytemuck::cast_slice(primitives),
        );

        // Check if we need to recreate the SDF bind group with actual glyph textures
        let atlas_view_ptr = atlas_view as *const wgpu::TextureView;
        let color_atlas_view_ptr = color_atlas_view as *const wgpu::TextureView;
        let need_new_bind_group = match &self.cached_sdf_with_glyphs {
            Some(cached) => {
                cached.atlas_view_ptr != atlas_view_ptr
                    || cached.color_atlas_view_ptr != color_atlas_view_ptr
            }
            None => true,
        };

        if need_new_bind_group {
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("SDF Bind Group with Glyphs"),
                layout: &self.bind_group_layouts.sdf,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.buffers.uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.buffers.primitives.as_entire_binding(),
                    },
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
                ],
            });
            self.cached_sdf_with_glyphs = Some(CachedSdfWithGlyphs {
                bind_group,
                atlas_view_ptr,
                color_atlas_view_ptr,
            });
        }

        let sdf_bind_group = &self.cached_sdf_with_glyphs.as_ref().unwrap().bind_group;

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Unified Primitives Encoder"),
            });

        // Begin render pass (load existing content)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Unified Primitives Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render SDF primitives (including text glyphs)
            render_pass.set_pipeline(&self.pipelines.sdf_overlay);
            render_pass.set_bind_group(0, sdf_bind_group, &[]);
            render_pass.draw(0..6, 0..primitives.len() as u32);
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
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

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [width as f32, height as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update primitives buffer
        if !batch.primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.primitives),
            );
        }

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

        // Pass 1: Render to MSAA texture with resolve
        // Use cached MSAA pipelines for sample_count > 1, otherwise fall back to base pipelines
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay MSAA Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &cached.msaa_view,
                    resolve_target: Some(&cached.resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Discard, // MSAA texture discarded after resolve
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Get the appropriate pipelines for the sample count
            let (path_pipeline, sdf_pipeline) = if sample_count > 1 {
                if let Some(ref msaa) = self.msaa_pipelines {
                    (&msaa.path, &msaa.sdf)
                } else {
                    // Fallback (shouldn't happen due to creation above)
                    (&self.pipelines.path, &self.pipelines.sdf)
                }
            } else {
                (&self.pipelines.path, &self.pipelines.sdf)
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

            // Render SDF primitives using MSAA pipeline
            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(sdf_pipeline);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
            }
        }

        // Pass 2: Blend resolved texture onto target using cached resources
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay Blend Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
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

        // Update glyphs buffer
        self.queue
            .write_buffer(&self.buffers.glyphs, 0, bytemuck::cast_slice(glyphs));

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
                        resource: self.buffers.glyphs.as_entire_binding(),
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
                            // params (border_radius, opacity, padding, padding)
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
            render_pass.draw(0..6, 0..instances.len() as u32);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}

impl Default for GpuRenderer {
    fn default() -> Self {
        // Create a basic renderer synchronously using pollster
        pollster::block_on(Self::new(RendererConfig::default()))
            .expect("Failed to create default renderer")
    }
}
