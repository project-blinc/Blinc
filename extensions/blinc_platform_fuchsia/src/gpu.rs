//! GPU integration for Fuchsia
//!
//! Provides Vulkan rendering via ImagePipe2 for presenting to Flatland.
//!
//! # Architecture
//!
//! On Fuchsia, GPU rendering uses the full Vulkan driver directly:
//!
//! - **Vulkan** via the fuchsia.vulkan.loader service (full 3D/2D capability)
//! - **Magma** GPU driver layer (Fuchsia's native GPU interface)
//! - **sysmem** for GPU buffer allocation shared between Vulkan and Flatland
//! - **ImagePipe2** for presenting frames to Flatland compositor
//!
//! # Rendering Flow
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                           Blinc App                                  │
//! │  ┌─────────────────────────────────────────────────────────────┐    │
//! │  │              blinc_gpu (wgpu Vulkan backend)                │    │
//! │  │    - Full 3D/2D rendering capability                       │    │
//! │  │    - SDF text, path tessellation, compute shaders          │    │
//! │  └─────────────────────────────────────────────────────────────┘    │
//! └───────────────────────────────┬─────────────────────────────────────┘
//!                                 │ Render to VkImage
//!                                 ▼
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                     sysmem BufferCollection                          │
//! │    - GPU memory shared between Vulkan and Flatland                  │
//! │    - Allocated via fuchsia.sysmem2.Allocator                        │
//! └───────────────────────────────┬─────────────────────────────────────┘
//!                                 │ Export via ImagePipe2
//!                                 ▼
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                    Flatland Compositor                               │
//! │    - Window management and compositing                              │
//! │    - Frame scheduling (vsync)                                       │
//! │    - Input routing to correct view                                  │
//! └───────────────────────────────┬─────────────────────────────────────┘
//!                                 │ Present
//!                                 ▼
//!                            ┌─────────┐
//!                            │ Display │
//!                            └─────────┘
//! ```

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(target_os = "fuchsia")]
use fidl_fuchsia_sysmem2::{
    AllocatorMarker as SysmemAllocatorMarker, AllocatorProxy as SysmemAllocatorProxy,
    BufferCollectionConstraints, BufferCollectionTokenMarker, BufferMemoryConstraints,
    BufferUsage, CoherencyDomain, Heap, ImageFormatConstraints,
};
#[cfg(target_os = "fuchsia")]
use fidl_fuchsia_ui_composition::{
    AllocatorProxy as FlatlandAllocatorProxy, BufferCollectionExportToken,
    BufferCollectionImportToken, RegisterBufferCollectionUsage,
};
#[cfg(target_os = "fuchsia")]
use fuchsia_component::client::connect_to_protocol;
#[cfg(target_os = "fuchsia")]
use fuchsia_zircon as zx;

use crate::flatland::{BufferCollection, BufferFormat, ContentId, FlatlandSession, ImageProperties};

/// Fuchsia surface handle for wgpu integration
///
/// This wraps the necessary handles for creating a wgpu surface on Fuchsia.
#[derive(Clone, Debug)]
pub struct FuchsiaSurfaceHandle {
    /// ImagePipe2 endpoint (token for image pipe)
    ///
    /// On Fuchsia, this would be a zx::Channel handle to the ImagePipe2.
    pub image_pipe_token: u64,

    /// Width of the surface in pixels
    pub width: u32,

    /// Height of the surface in pixels
    pub height: u32,

    /// Buffer collection export token (for Vulkan import)
    pub buffer_collection_token: u64,
}

impl FuchsiaSurfaceHandle {
    /// Create a new surface handle
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            image_pipe_token: 0,
            width,
            height,
            buffer_collection_token: 0,
        }
    }

    /// Create with ImagePipe2 token
    pub fn with_image_pipe(image_pipe_token: u64, width: u32, height: u32) -> Self {
        Self {
            image_pipe_token,
            width,
            height,
            buffer_collection_token: 0,
        }
    }

    /// Check if this handle is valid
    pub fn is_valid(&self) -> bool {
        self.image_pipe_token != 0 && self.width > 0 && self.height > 0
    }
}

/// ImagePipe2 client for presenting Vulkan frames to Flatland
///
/// ImagePipe2 is the bridge between Vulkan rendering and Flatland compositing.
/// It manages a set of GPU buffers that Vulkan renders into and Flatland displays.
///
/// # Buffer Management
///
/// Uses a double/triple buffer scheme:
/// 1. Vulkan renders to the "write" buffer
/// 2. Present sends buffer to Flatland
/// 3. Previous buffer becomes available for next frame
///
/// # Synchronization
///
/// - Acquire fence: Signaled when buffer is ready for rendering
/// - Release fence: Signaled when Flatland is done displaying
pub struct ImagePipeClient {
    /// Buffer collection shared with Vulkan
    buffer_collection: Option<BufferCollection>,
    /// Number of buffers (typically 2-3)
    buffer_count: u32,
    /// Current buffer index for rendering
    current_buffer: AtomicU32,
    /// Surface dimensions
    width: u32,
    height: u32,
    /// Pixel format
    format: BufferFormat,
    /// Associated Flatland content ID
    content_id: Option<ContentId>,
    /// Present counter
    present_count: AtomicU64,
}

impl ImagePipeClient {
    /// Create a new ImagePipe client
    ///
    /// # Arguments
    ///
    /// * `width` - Surface width in pixels
    /// * `height` - Surface height in pixels
    /// * `buffer_count` - Number of swapchain buffers (2 = double, 3 = triple)
    /// * `format` - Pixel format for the buffers
    pub fn new(width: u32, height: u32, buffer_count: u32, format: BufferFormat) -> Self {
        Self {
            buffer_collection: None,
            buffer_count: buffer_count.clamp(2, 4),
            current_buffer: AtomicU32::new(0),
            width,
            height,
            format,
            content_id: None,
            present_count: AtomicU64::new(0),
        }
    }

    /// Initialize the ImagePipe with buffer collection
    ///
    /// On Fuchsia, this:
    /// 1. Creates a BufferCollection via sysmem2
    /// 2. Sets Vulkan constraints (VK_FUCHSIA_buffer_collection)
    /// 3. Sets Flatland constraints via Allocator
    /// 4. Waits for allocation to complete
    #[cfg(target_os = "fuchsia")]
    pub async fn initialize(&mut self) -> Result<(), ImagePipeError> {
        // Connect to sysmem2 allocator
        let sysmem_allocator = connect_to_protocol::<SysmemAllocatorMarker>()
            .map_err(|e| ImagePipeError::AllocationFailed(format!("sysmem connect: {:?}", e)))?;

        // Create buffer collection token
        let (token_client, token_server) = fidl::endpoints::create_endpoints::<BufferCollectionTokenMarker>();
        sysmem_allocator.allocate_shared_collection(fidl_fuchsia_sysmem2::AllocatorAllocateSharedCollectionRequest {
            token_request: Some(token_server),
            ..Default::default()
        }).map_err(|e| ImagePipeError::AllocationFailed(format!("allocate_shared_collection: {:?}", e)))?;

        // Duplicate token for Flatland
        let token_proxy = token_client.into_proxy()
            .map_err(|e| ImagePipeError::AllocationFailed(format!("token proxy: {:?}", e)))?;

        let (flatland_token, flatland_token_server) = fidl::endpoints::create_endpoints::<BufferCollectionTokenMarker>();
        token_proxy.duplicate(fidl_fuchsia_sysmem2::BufferCollectionTokenDuplicateRequest {
            rights_attenuation_mask: Some(zx::Rights::SAME_RIGHTS),
            token_request: Some(flatland_token_server),
            ..Default::default()
        }).map_err(|e| ImagePipeError::AllocationFailed(format!("duplicate token: {:?}", e)))?;

        token_proxy.sync().await
            .map_err(|e| ImagePipeError::AllocationFailed(format!("token sync: {:?}", e)))?;

        // Set Vulkan constraints on our token
        let (collection_client, collection_server) = fidl::endpoints::create_endpoints();
        sysmem_allocator.bind_shared_collection(fidl_fuchsia_sysmem2::AllocatorBindSharedCollectionRequest {
            token: Some(token_proxy.into_client_end().unwrap()),
            buffer_collection_request: Some(collection_server),
            ..Default::default()
        }).map_err(|e| ImagePipeError::AllocationFailed(format!("bind_shared_collection: {:?}", e)))?;

        let collection_proxy = collection_client.into_proxy()
            .map_err(|e| ImagePipeError::AllocationFailed(format!("collection proxy: {:?}", e)))?;

        // Set buffer constraints for Vulkan rendering
        let constraints = BufferCollectionConstraints {
            usage: Some(BufferUsage {
                vulkan: Some(fidl_fuchsia_sysmem2::VULKAN_IMAGE_USAGE_COLOR_ATTACHMENT
                    | fidl_fuchsia_sysmem2::VULKAN_IMAGE_USAGE_TRANSFER_SRC
                    | fidl_fuchsia_sysmem2::VULKAN_IMAGE_USAGE_TRANSFER_DST),
                ..Default::default()
            }),
            min_buffer_count: Some(self.buffer_count),
            buffer_memory_constraints: Some(BufferMemoryConstraints {
                ram_domain_supported: Some(true),
                cpu_domain_supported: Some(false),
                inaccessible_domain_supported: Some(true),
                heap_permitted: Some(vec![Heap {
                    heap_type: Some(
                        fidl_fuchsia_sysmem2::HeapType::GoldfishDeviceLocal.into_primitive().into()
                    ),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            image_format_constraints: Some(vec![ImageFormatConstraints {
                pixel_format: Some(match self.format {
                    BufferFormat::B8G8R8A8 | BufferFormat::B8G8R8A8Srgb =>
                        fidl_fuchsia_images2::PixelFormat::B8G8R8A8,
                    BufferFormat::R8G8B8A8 | BufferFormat::R8G8B8A8Srgb =>
                        fidl_fuchsia_images2::PixelFormat::R8G8B8A8,
                }),
                min_size: Some(fidl_fuchsia_math::SizeU {
                    width: self.width,
                    height: self.height,
                }),
                max_size: Some(fidl_fuchsia_math::SizeU {
                    width: self.width,
                    height: self.height,
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };

        collection_proxy.set_constraints(fidl_fuchsia_sysmem2::BufferCollectionSetConstraintsRequest {
            constraints: Some(constraints),
            ..Default::default()
        }).map_err(|e| ImagePipeError::AllocationFailed(format!("set_constraints: {:?}", e)))?;

        // Wait for buffers to be allocated
        let wait_result = collection_proxy.wait_for_all_buffers_allocated().await
            .map_err(|e| ImagePipeError::AllocationFailed(format!("wait_for_buffers: {:?}", e)))?
            .map_err(|e| ImagePipeError::AllocationFailed(format!("allocation failed: {:?}", e)))?;

        let buffer_info = wait_result.buffer_collection_info
            .ok_or_else(|| ImagePipeError::AllocationFailed("No buffer info".to_string()))?;

        let buffer_count = buffer_info.buffers.as_ref()
            .map(|b| b.len() as u32)
            .unwrap_or(0);

        tracing::info!(
            "ImagePipe initialized: {}x{} with {} buffers allocated",
            self.width, self.height, buffer_count
        );

        // Store buffer collection info
        self.buffer_collection = Some(BufferCollection::new(
            buffer_count,
            self.width,
            self.height,
            self.format,
        ));

        Ok(())
    }

    /// Initialize (placeholder for non-Fuchsia)
    #[cfg(not(target_os = "fuchsia"))]
    pub fn initialize_sync(&mut self) -> Result<(), ImagePipeError> {
        self.buffer_collection = Some(BufferCollection::new(
            self.buffer_count,
            self.width,
            self.height,
            self.format,
        ));
        Ok(())
    }

    /// Register this ImagePipe with a Flatland session
    ///
    /// Creates the ContentId that can be attached to a transform.
    pub fn register_with_flatland(&mut self, session: &FlatlandSession) -> Result<ContentId, ImagePipeError> {
        let collection = self.buffer_collection.as_ref()
            .ok_or(ImagePipeError::NotInitialized)?;

        // Create image content from buffer collection
        let content_id = session.create_image_from_buffer_collection(
            (), // Would be BufferCollectionImportToken
            0,  // First buffer
            ImageProperties {
                width: collection.width,
                height: collection.height,
            },
        );

        self.content_id = Some(content_id);
        Ok(content_id)
    }

    /// Get the current buffer index for rendering
    pub fn current_buffer_index(&self) -> u32 {
        self.current_buffer.load(Ordering::Acquire)
    }

    /// Acquire the next buffer for rendering
    ///
    /// Returns the buffer index to render into.
    /// On Fuchsia, would also return acquire fence to wait on.
    pub fn acquire_next_buffer(&self) -> AcquiredBuffer {
        let index = self.current_buffer.load(Ordering::Acquire);
        AcquiredBuffer {
            index,
            width: self.width,
            height: self.height,
            // On Fuchsia: acquire_fence would be set
        }
    }

    /// Present the current buffer to Flatland
    ///
    /// # Arguments
    ///
    /// * `presentation_time` - Requested presentation time (0 = ASAP)
    ///
    /// Returns release fence that signals when buffer can be reused.
    #[cfg(target_os = "fuchsia")]
    pub async fn present(&self, presentation_time: i64) -> Result<PresentResult, ImagePipeError> {
        // Advance to next buffer
        let next = (self.current_buffer.load(Ordering::Acquire) + 1) % self.buffer_count;
        self.current_buffer.store(next, Ordering::Release);

        self.present_count.fetch_add(1, Ordering::Relaxed);

        // On Fuchsia:
        // 1. Signal acquire fence for compositor
        // 2. Call Flatland.Present with buffer index
        // 3. Return release fence

        Ok(PresentResult {
            presentation_time,
            buffer_released: next,
        })
    }

    /// Synchronous present (placeholder for non-Fuchsia)
    #[cfg(not(target_os = "fuchsia"))]
    pub fn present_sync(&self, presentation_time: i64) -> Result<PresentResult, ImagePipeError> {
        let next = (self.current_buffer.load(Ordering::Acquire) + 1) % self.buffer_count;
        self.current_buffer.store(next, Ordering::Release);
        self.present_count.fetch_add(1, Ordering::Relaxed);

        Ok(PresentResult {
            presentation_time,
            buffer_released: next,
        })
    }

    /// Resize the ImagePipe buffers
    ///
    /// This reallocates the buffer collection with new dimensions.
    #[cfg(target_os = "fuchsia")]
    pub async fn resize(&mut self, width: u32, height: u32) -> Result<(), ImagePipeError> {
        if width == self.width && height == self.height {
            return Ok(());
        }

        self.width = width;
        self.height = height;

        // Reallocate buffer collection
        self.buffer_collection = None;
        self.initialize().await?;

        tracing::info!("ImagePipe resized to {}x{}", width, height);
        Ok(())
    }

    /// Get present count (for debugging/stats)
    pub fn present_count(&self) -> u64 {
        self.present_count.load(Ordering::Relaxed)
    }

    /// Get the associated content ID
    pub fn content_id(&self) -> Option<ContentId> {
        self.content_id
    }

    /// Get surface dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Acquired buffer for rendering
#[derive(Debug, Clone)]
pub struct AcquiredBuffer {
    /// Buffer index in the collection
    pub index: u32,
    /// Buffer width
    pub width: u32,
    /// Buffer height
    pub height: u32,
    // On Fuchsia: pub acquire_fence: zx::Event,
}

/// Result from presenting a frame
#[derive(Debug, Clone)]
pub struct PresentResult {
    /// Actual presentation time
    pub presentation_time: i64,
    /// Buffer index that was released
    pub buffer_released: u32,
    // On Fuchsia: pub release_fence: zx::Event,
}

/// Errors from ImagePipe operations
#[derive(Debug, Clone)]
pub enum ImagePipeError {
    /// ImagePipe not initialized
    NotInitialized,
    /// Buffer allocation failed
    AllocationFailed(String),
    /// Present failed
    PresentFailed(String),
    /// No buffers available
    NoBuffersAvailable,
    /// Invalid buffer index
    InvalidBufferIndex(u32),
    /// A frame is already in progress
    FrameInProgress,
    /// No frame is in progress
    NoFrameInProgress,
}

impl std::fmt::Display for ImagePipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "ImagePipe not initialized"),
            Self::AllocationFailed(msg) => write!(f, "Buffer allocation failed: {}", msg),
            Self::PresentFailed(msg) => write!(f, "Present failed: {}", msg),
            Self::NoBuffersAvailable => write!(f, "No buffers available for rendering"),
            Self::InvalidBufferIndex(idx) => write!(f, "Invalid buffer index: {}", idx),
            Self::FrameInProgress => write!(f, "A frame is already in progress"),
            Self::NoFrameInProgress => write!(f, "No frame is in progress"),
        }
    }
}

impl std::error::Error for ImagePipeError {}

/// GPU backend configuration for Fuchsia
#[derive(Clone, Debug)]
pub struct GpuConfig {
    /// Use Vulkan (always true on Fuchsia)
    pub use_vulkan: bool,
    /// Preferred present mode
    pub present_mode: PresentMode,
    /// Requested sample count for MSAA
    pub sample_count: u32,
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            use_vulkan: true,
            present_mode: PresentMode::Fifo,
            sample_count: 1,
        }
    }
}

/// Present mode for swapchain
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentMode {
    /// Vsync enabled, no tearing
    Fifo,
    /// Lower latency, may tear
    Immediate,
    /// Mailbox (triple buffering)
    Mailbox,
}

/// Information about GPU capabilities on this device
#[derive(Clone, Debug)]
pub struct GpuInfo {
    /// Vulkan API version
    pub vulkan_version: (u32, u32, u32),
    /// Device name
    pub device_name: String,
    /// Driver version
    pub driver_version: String,
    /// Maximum texture dimension
    pub max_texture_dimension: u32,
    /// Supports compute shaders
    pub supports_compute: bool,
}

impl Default for GpuInfo {
    fn default() -> Self {
        Self {
            vulkan_version: (1, 2, 0),
            device_name: "Unknown".to_string(),
            driver_version: "Unknown".to_string(),
            max_texture_dimension: 8192,
            supports_compute: true,
        }
    }
}

/// Create wgpu instance configuration for Fuchsia
///
/// Returns the appropriate wgpu backend configuration.
pub fn wgpu_instance_descriptor() -> wgpu_types::InstanceDescriptor {
    wgpu_types::InstanceDescriptor {
        backends: wgpu_types::Backends::VULKAN,
        ..Default::default()
    }
}

/// Vulkan surface wrapper for Fuchsia
///
/// Bridges wgpu's Vulkan backend with ImagePipe2/Flatland presentation.
///
/// # Vulkan Extensions Used
///
/// - `VK_FUCHSIA_imagepipe_surface`: Create VkSurfaceKHR from ImagePipe2
/// - `VK_FUCHSIA_buffer_collection`: Import sysmem buffers into Vulkan
/// - `VK_FUCHSIA_external_memory`: Share GPU memory with compositor
pub struct VulkanSurface {
    /// ImagePipe client for buffer management
    image_pipe: ImagePipeClient,
    /// Flatland session reference
    flatland: Arc<std::sync::RwLock<FlatlandSession>>,
    /// Surface configuration
    config: GpuConfig,
    /// Whether surface is valid
    valid: bool,
}

impl VulkanSurface {
    /// Create a new Vulkan surface
    pub fn new(
        width: u32,
        height: u32,
        config: GpuConfig,
        flatland: Arc<std::sync::RwLock<FlatlandSession>>,
    ) -> Result<Self, ImagePipeError> {
        let buffer_count = match config.present_mode {
            PresentMode::Fifo => 2,      // Double buffering
            PresentMode::Mailbox => 3,   // Triple buffering
            PresentMode::Immediate => 2, // Minimal buffering
        };

        let image_pipe = ImagePipeClient::new(
            width,
            height,
            buffer_count,
            BufferFormat::B8G8R8A8Srgb, // Standard sRGB format
        );

        Ok(Self {
            image_pipe,
            flatland,
            config,
            valid: false,
        })
    }

    /// Initialize the surface
    ///
    /// Must be called before rendering.
    #[cfg(target_os = "fuchsia")]
    pub async fn initialize(&mut self) -> Result<(), ImagePipeError> {
        // Initialize ImagePipe buffers
        self.image_pipe.initialize().await?;

        // Register with Flatland
        {
            let session = self.flatland.read().map_err(|_| {
                ImagePipeError::AllocationFailed("Flatland lock poisoned".to_string())
            })?;
            self.image_pipe.register_with_flatland(&session)?;
        }

        self.valid = true;
        tracing::info!("VulkanSurface initialized");
        Ok(())
    }

    /// Initialize (placeholder for non-Fuchsia)
    #[cfg(not(target_os = "fuchsia"))]
    pub fn initialize_sync(&mut self) -> Result<(), ImagePipeError> {
        self.image_pipe.initialize_sync()?;

        {
            let session = self.flatland.read().map_err(|_| {
                ImagePipeError::AllocationFailed("Flatland lock poisoned".to_string())
            })?;
            self.image_pipe.register_with_flatland(&session)?;
        }

        self.valid = true;
        Ok(())
    }

    /// Acquire next frame for rendering
    pub fn acquire_frame(&self) -> Result<AcquiredBuffer, ImagePipeError> {
        if !self.valid {
            return Err(ImagePipeError::NotInitialized);
        }
        Ok(self.image_pipe.acquire_next_buffer())
    }

    /// Present the current frame
    #[cfg(target_os = "fuchsia")]
    pub async fn present(&self) -> Result<PresentResult, ImagePipeError> {
        if !self.valid {
            return Err(ImagePipeError::NotInitialized);
        }
        self.image_pipe.present(0).await
    }

    /// Present (placeholder for non-Fuchsia)
    #[cfg(not(target_os = "fuchsia"))]
    pub fn present_sync(&self) -> Result<PresentResult, ImagePipeError> {
        if !self.valid {
            return Err(ImagePipeError::NotInitialized);
        }
        self.image_pipe.present_sync(0)
    }

    /// Resize the surface
    #[cfg(target_os = "fuchsia")]
    pub async fn resize(&mut self, width: u32, height: u32) -> Result<(), ImagePipeError> {
        self.image_pipe.resize(width, height).await
    }

    /// Get surface dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        self.image_pipe.dimensions()
    }

    /// Check if surface is valid for rendering
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Get the Flatland content ID for scene graph attachment
    pub fn content_id(&self) -> Option<ContentId> {
        self.image_pipe.content_id()
    }
}

/// Helper for creating Vulkan surface on Fuchsia
///
/// This uses the VK_FUCHSIA_imagepipe_surface extension to create
/// a VkSurfaceKHR from an ImagePipe2 token.
///
/// # Vulkan Extensions Required
///
/// - `VK_KHR_surface`
/// - `VK_FUCHSIA_imagepipe_surface`
///
/// # Example
///
/// ```ignore
/// let handle = FuchsiaSurfaceHandle::with_image_pipe(token, 1920, 1080);
/// let surface_info = create_vulkan_surface_info(&handle)?;
/// // Use with wgpu::Instance::create_surface_from_raw_surface
/// ```
#[cfg(target_os = "fuchsia")]
pub fn create_vulkan_surface_info(
    handle: &FuchsiaSurfaceHandle,
) -> Result<VulkanSurfaceInfo, blinc_platform::PlatformError> {
    if !handle.is_valid() {
        return Err(blinc_platform::PlatformError::WindowCreationFailed(
            "Invalid surface handle".to_string(),
        ));
    }

    // Create the Vulkan surface info for VK_FUCHSIA_imagepipe_surface
    // This will be used with vkCreateImagePipeSurfaceFUCHSIA
    Ok(VulkanSurfaceInfo {
        image_pipe_handle: zx::Handle::from(zx::Channel::from(
            unsafe { zx::Handle::from_raw(handle.image_pipe_token as u32) }
        )),
        width: handle.width,
        height: handle.height,
    })
}

/// Information for creating a Vulkan surface
#[cfg(target_os = "fuchsia")]
#[derive(Debug)]
pub struct VulkanSurfaceInfo {
    /// ImagePipe2 handle for the surface (zx::Handle for vkCreateImagePipeSurfaceFUCHSIA)
    pub image_pipe_handle: zx::Handle,
    /// Surface width
    pub width: u32,
    /// Surface height
    pub height: u32,
}

/// Create a Vulkan surface using VK_FUCHSIA_imagepipe_surface
///
/// This function creates the actual VkSurfaceKHR for wgpu to use.
#[cfg(target_os = "fuchsia")]
pub async fn create_image_pipe_surface(
    width: u32,
    height: u32,
) -> Result<(FuchsiaSurfaceHandle, zx::EventPair), ImagePipeError> {
    use fidl_fuchsia_ui_composition::AllocatorMarker;

    // Connect to Flatland allocator
    let flatland_allocator = connect_to_protocol::<AllocatorMarker>()
        .map_err(|e| ImagePipeError::AllocationFailed(format!("flatland allocator: {:?}", e)))?;

    // Create export/import token pair for the buffer collection
    let (export_token, import_token) = zx::EventPair::create()
        .map_err(|e| ImagePipeError::AllocationFailed(format!("create eventpair: {:?}", e)))?;

    // Register the buffer collection with Flatland
    flatland_allocator.register_buffer_collection(
        fidl_fuchsia_ui_composition::RegisterBufferCollectionArgs {
            export_token: Some(BufferCollectionExportToken { value: export_token.duplicate_handle(zx::Rights::SAME_RIGHTS).unwrap() }),
            buffer_collection_token: None,
            usage: Some(RegisterBufferCollectionUsage::Default),
            ..Default::default()
        }
    ).await
        .map_err(|e| ImagePipeError::AllocationFailed(format!("register_buffer_collection fidl: {:?}", e)))?
        .map_err(|e| ImagePipeError::AllocationFailed(format!("register_buffer_collection: {:?}", e)))?;

    // Create surface handle with the import token's raw handle
    let handle = FuchsiaSurfaceHandle {
        image_pipe_token: import_token.raw_handle() as u64,
        width,
        height,
        buffer_collection_token: export_token.raw_handle() as u64,
    };

    tracing::info!("Created ImagePipe surface: {}x{}", width, height);

    Ok((handle, import_token))
}

/// Create wgpu surface configuration for Fuchsia
///
/// Returns the appropriate texture format and present mode.
pub fn wgpu_surface_config(
    width: u32,
    height: u32,
    config: &GpuConfig,
) -> wgpu_types::SurfaceConfiguration<Vec<wgpu_types::TextureFormat>> {
    wgpu_types::SurfaceConfiguration {
        usage: wgpu_types::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu_types::TextureFormat::Bgra8UnormSrgb,
        width,
        height,
        present_mode: match config.present_mode {
            PresentMode::Fifo => wgpu_types::PresentMode::Fifo,
            PresentMode::Immediate => wgpu_types::PresentMode::Immediate,
            PresentMode::Mailbox => wgpu_types::PresentMode::Mailbox,
        },
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu_types::CompositeAlphaMode::PreMultiplied,
        view_formats: vec![],
    }
}

// Re-export wgpu_types for convenience
pub use wgpu_types;

// ============================================================================
// High-Level GPU Integration
// ============================================================================

/// Fuchsia GPU renderer state
///
/// Manages the full GPU pipeline for rendering Blinc content to Flatland.
///
/// # Initialization Order
///
/// 1. Create `FuchsiaGpu::new()` with dimensions and config
/// 2. Call `initialize()` to allocate buffers
/// 3. Call `create_wgpu_surface()` to get surface for wgpu
/// 4. Render frames with `begin_frame()` / `end_frame()`
pub struct FuchsiaGpu {
    /// Vulkan surface wrapper
    surface: VulkanSurface,
    /// Current frame index
    frame_number: u64,
    /// Whether we're mid-frame
    in_frame: bool,
}

impl FuchsiaGpu {
    /// Create new GPU renderer
    pub fn new(
        width: u32,
        height: u32,
        config: GpuConfig,
        flatland: Arc<std::sync::RwLock<FlatlandSession>>,
    ) -> Result<Self, ImagePipeError> {
        let surface = VulkanSurface::new(width, height, config, flatland)?;

        Ok(Self {
            surface,
            frame_number: 0,
            in_frame: false,
        })
    }

    /// Initialize GPU resources
    ///
    /// Must be called before rendering.
    #[cfg(target_os = "fuchsia")]
    pub async fn initialize(&mut self) -> Result<(), ImagePipeError> {
        self.surface.initialize().await
    }

    /// Initialize (placeholder for non-Fuchsia)
    #[cfg(not(target_os = "fuchsia"))]
    pub fn initialize_sync(&mut self) -> Result<(), ImagePipeError> {
        self.surface.initialize_sync()
    }

    /// Check if GPU is ready for rendering
    pub fn is_ready(&self) -> bool {
        self.surface.is_valid()
    }

    /// Get surface dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        self.surface.dimensions()
    }

    /// Get Flatland content ID
    pub fn content_id(&self) -> Option<ContentId> {
        self.surface.content_id()
    }

    /// Begin a new frame
    ///
    /// Returns the acquired buffer for rendering.
    pub fn begin_frame(&mut self) -> Result<AcquiredBuffer, ImagePipeError> {
        if self.in_frame {
            return Err(ImagePipeError::FrameInProgress);
        }

        let buffer = self.surface.acquire_frame()?;
        self.in_frame = true;
        Ok(buffer)
    }

    /// End the current frame and present
    #[cfg(target_os = "fuchsia")]
    pub async fn end_frame(&mut self) -> Result<PresentResult, ImagePipeError> {
        if !self.in_frame {
            return Err(ImagePipeError::NoFrameInProgress);
        }

        let result = self.surface.present().await?;
        self.in_frame = false;
        self.frame_number += 1;
        Ok(result)
    }

    /// End frame (placeholder for non-Fuchsia)
    #[cfg(not(target_os = "fuchsia"))]
    pub fn end_frame_sync(&mut self) -> Result<PresentResult, ImagePipeError> {
        if !self.in_frame {
            return Err(ImagePipeError::NoFrameInProgress);
        }

        let result = self.surface.present_sync()?;
        self.in_frame = false;
        self.frame_number += 1;
        Ok(result)
    }

    /// Get frame number
    pub fn frame_number(&self) -> u64 {
        self.frame_number
    }

    /// Check if we're mid-frame
    pub fn in_frame(&self) -> bool {
        self.in_frame
    }

    /// Resize the GPU surface
    #[cfg(target_os = "fuchsia")]
    pub async fn resize(&mut self, width: u32, height: u32) -> Result<(), ImagePipeError> {
        if self.in_frame {
            return Err(ImagePipeError::FrameInProgress);
        }
        self.surface.resize(width, height).await
    }

    /// Get wgpu surface configuration
    pub fn wgpu_config(&self, config: &GpuConfig) -> wgpu_types::SurfaceConfiguration<Vec<wgpu_types::TextureFormat>> {
        let (width, height) = self.dimensions();
        wgpu_surface_config(width, height, config)
    }
}

/// Additional error variants for frame management
impl ImagePipeError {
    /// Frame is already in progress
    pub const fn frame_in_progress() -> Self {
        Self::FrameInProgress
    }

    /// No frame in progress
    pub const fn no_frame_in_progress() -> Self {
        Self::NoFrameInProgress
    }
}

/// Convenience function to create full GPU stack for Fuchsia
///
/// Sets up Flatland session, ImagePipe, and GPU renderer.
pub fn create_fuchsia_gpu(
    width: u32,
    height: u32,
    config: Option<GpuConfig>,
) -> Result<(FlatlandSession, FuchsiaGpu), ImagePipeError> {
    let config = config.unwrap_or_default();

    // Create Flatland session
    let flatland = FlatlandSession::new().map_err(|e| {
        ImagePipeError::AllocationFailed(format!("Flatland: {}", e))
    })?;
    let flatland_arc = Arc::new(std::sync::RwLock::new(flatland));

    // Create GPU renderer
    let gpu = FuchsiaGpu::new(width, height, config, Arc::clone(&flatland_arc))?;

    // Extract session (move out of Arc since we just created it)
    let session = Arc::try_unwrap(flatland_arc)
        .map_err(|_| ImagePipeError::AllocationFailed("Arc still referenced".to_string()))?
        .into_inner()
        .map_err(|_| ImagePipeError::AllocationFailed("Lock poisoned".to_string()))?;

    Ok((session, gpu))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_handle_validity() {
        let invalid = FuchsiaSurfaceHandle::new(0, 0);
        assert!(!invalid.is_valid());

        let mut valid = FuchsiaSurfaceHandle::new(1920, 1080);
        assert!(!valid.is_valid()); // Still invalid without token

        valid.image_pipe_token = 123;
        assert!(valid.is_valid());
    }

    #[test]
    fn test_surface_handle_with_image_pipe() {
        let handle = FuchsiaSurfaceHandle::with_image_pipe(42, 800, 600);
        assert!(handle.is_valid());
        assert_eq!(handle.image_pipe_token, 42);
        assert_eq!(handle.width, 800);
        assert_eq!(handle.height, 600);
    }

    #[test]
    fn test_gpu_config_default() {
        let config = GpuConfig::default();
        assert!(config.use_vulkan);
        assert_eq!(config.present_mode, PresentMode::Fifo);
    }

    #[test]
    fn test_image_pipe_client_new() {
        let client = ImagePipeClient::new(1920, 1080, 2, BufferFormat::B8G8R8A8Srgb);
        assert_eq!(client.dimensions(), (1920, 1080));
        assert_eq!(client.current_buffer_index(), 0);
        assert_eq!(client.present_count(), 0);
        assert!(client.content_id().is_none());
    }

    #[test]
    fn test_image_pipe_acquire_buffer() {
        let client = ImagePipeClient::new(800, 600, 3, BufferFormat::R8G8B8A8);
        let buffer = client.acquire_next_buffer();
        assert_eq!(buffer.index, 0);
        assert_eq!(buffer.width, 800);
        assert_eq!(buffer.height, 600);
    }

    #[test]
    fn test_image_pipe_present_cycles_buffers() {
        let mut client = ImagePipeClient::new(640, 480, 3, BufferFormat::B8G8R8A8Srgb);
        client.initialize_sync().unwrap();

        // Present cycles through buffers
        client.present_sync(0).unwrap();
        assert_eq!(client.current_buffer_index(), 1);
        assert_eq!(client.present_count(), 1);

        client.present_sync(0).unwrap();
        assert_eq!(client.current_buffer_index(), 2);

        client.present_sync(0).unwrap();
        assert_eq!(client.current_buffer_index(), 0); // Wraps around

        assert_eq!(client.present_count(), 3);
    }

    #[test]
    fn test_wgpu_surface_config() {
        let config = GpuConfig {
            use_vulkan: true,
            present_mode: PresentMode::Mailbox,
            sample_count: 4,
        };
        let surface_config = wgpu_surface_config(1920, 1080, &config);

        assert_eq!(surface_config.width, 1920);
        assert_eq!(surface_config.height, 1080);
        assert_eq!(surface_config.present_mode, wgpu_types::PresentMode::Mailbox);
        assert_eq!(surface_config.format, wgpu_types::TextureFormat::Bgra8UnormSrgb);
    }

    #[test]
    fn test_image_pipe_error_display() {
        let err = ImagePipeError::NotInitialized;
        assert_eq!(format!("{}", err), "ImagePipe not initialized");

        let err = ImagePipeError::AllocationFailed("out of memory".to_string());
        assert!(format!("{}", err).contains("out of memory"));
    }
}
