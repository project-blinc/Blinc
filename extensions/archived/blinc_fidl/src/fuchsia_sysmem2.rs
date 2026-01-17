//! Stub types for fuchsia.sysmem2 FIDL library
//!
//! These provide API compatibility for cross-compilation.
//! On actual Fuchsia, use the generated fidl_fuchsia_sysmem2 crate.

use crate::fuchsia_images2::PixelFormat;
use crate::fuchsia_math::SizeU;

/// Vulkan buffer usage flags
pub const VULKAN_IMAGE_USAGE_COLOR_ATTACHMENT: u32 = 0x0000_0010;
pub const VULKAN_IMAGE_USAGE_TRANSFER_SRC: u32 = 0x0000_0001;
pub const VULKAN_IMAGE_USAGE_TRANSFER_DST: u32 = 0x0000_0002;
pub const VULKAN_IMAGE_USAGE_SAMPLED: u32 = 0x0000_0004;

/// Buffer usage specification
#[derive(Clone, Debug, Default)]
pub struct BufferUsage {
    /// No usage flags
    pub none: Option<u32>,
    /// CPU usage flags
    pub cpu: Option<u32>,
    /// Vulkan usage flags
    pub vulkan: Option<u32>,
    /// Display usage flags
    pub display: Option<u32>,
    /// Video usage flags
    pub video: Option<u32>,
}

/// Memory coherency domain
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum CoherencyDomain {
    /// CPU domain
    Cpu = 0,
    /// RAM domain (cached)
    Ram = 1,
    /// Inaccessible (GPU-only)
    Inaccessible = 2,
}

impl Default for CoherencyDomain {
    fn default() -> Self {
        Self::Ram
    }
}

/// Heap type for memory allocation
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u64)]
pub enum HeapType {
    /// System RAM
    SystemRam = 0x0000_0000_0000_0000,
    /// Goldfish host memory (emulator)
    GoldfishHostVisible = 0x6F66_6C64_6973_6801,
    /// Goldfish device-local memory
    GoldfishDeviceLocal = 0x6F66_6C64_6973_6802,
    /// AMLOGIC secure heap
    AmlogicSecure = 0x616D_6C6F_6769_6301,
    /// Framebuffer
    Framebuffer = 0x0000_0000_0000_0001,
}

impl HeapType {
    /// Convert to primitive value
    pub fn into_primitive(self) -> u64 {
        self as u64
    }
}

impl Default for HeapType {
    fn default() -> Self {
        Self::SystemRam
    }
}

/// Heap specification
#[derive(Clone, Debug, Default)]
pub struct Heap {
    /// Heap type
    pub heap_type: Option<u64>,
    /// Heap ID (optional)
    pub id: Option<u64>,
}

/// Buffer memory constraints
#[derive(Clone, Debug, Default)]
pub struct BufferMemoryConstraints {
    /// Minimum size in bytes
    pub min_size_bytes: Option<u64>,
    /// Maximum size in bytes
    pub max_size_bytes: Option<u64>,
    /// Whether physically contiguous memory is required
    pub physically_contiguous_required: Option<bool>,
    /// Whether secure memory is required
    pub secure_required: Option<bool>,
    /// Whether RAM domain is supported
    pub ram_domain_supported: Option<bool>,
    /// Whether CPU domain is supported
    pub cpu_domain_supported: Option<bool>,
    /// Whether inaccessible domain is supported
    pub inaccessible_domain_supported: Option<bool>,
    /// Permitted heaps
    pub heap_permitted: Option<Vec<Heap>>,
}

/// Image format constraints
#[derive(Clone, Debug, Default)]
pub struct ImageFormatConstraints {
    /// Pixel format
    pub pixel_format: Option<PixelFormat>,
    /// Modifier (for tiling, etc.)
    pub pixel_format_modifier: Option<u64>,
    /// Minimum coded size
    pub min_size: Option<SizeU>,
    /// Maximum coded size
    pub max_size: Option<SizeU>,
    /// Minimum bytes per row
    pub min_bytes_per_row: Option<u32>,
    /// Maximum bytes per row
    pub max_bytes_per_row: Option<u32>,
    /// Maximum width times height
    pub max_width_times_height: Option<u64>,
    /// Required size alignment
    pub size_alignment: Option<SizeU>,
    /// Display rect alignment
    pub display_rect_alignment: Option<SizeU>,
    /// Required row stride alignment
    pub bytes_per_row_divisor: Option<u32>,
    /// Start offset alignment
    pub start_offset_divisor: Option<u32>,
}

/// Buffer collection constraints
#[derive(Clone, Debug, Default)]
pub struct BufferCollectionConstraints {
    /// Buffer usage
    pub usage: Option<BufferUsage>,
    /// Minimum buffer count
    pub min_buffer_count: Option<u32>,
    /// Maximum buffer count
    pub max_buffer_count: Option<u32>,
    /// Buffer memory constraints
    pub buffer_memory_constraints: Option<BufferMemoryConstraints>,
    /// Image format constraints
    pub image_format_constraints: Option<Vec<ImageFormatConstraints>>,
}

/// Request for allocating a shared collection
#[derive(Clone, Debug, Default)]
pub struct AllocatorAllocateSharedCollectionRequest {
    /// Token server end
    pub token_request: Option<()>, // Would be ServerEnd<BufferCollectionTokenMarker>
}

/// Request for binding a shared collection
#[derive(Clone, Debug, Default)]
pub struct AllocatorBindSharedCollectionRequest {
    /// Token client end
    pub token: Option<()>, // Would be ClientEnd<BufferCollectionTokenMarker>
    /// Buffer collection request
    pub buffer_collection_request: Option<()>, // Would be ServerEnd<BufferCollectionMarker>
}

/// Request for setting constraints on a buffer collection
#[derive(Clone, Debug, Default)]
pub struct BufferCollectionSetConstraintsRequest {
    /// Constraints to set
    pub constraints: Option<BufferCollectionConstraints>,
}

/// Request for duplicating a token
#[derive(Clone, Debug, Default)]
pub struct BufferCollectionTokenDuplicateRequest {
    /// Rights mask
    pub rights_attenuation_mask: Option<u32>,
    /// Token server end
    pub token_request: Option<()>, // Would be ServerEnd<BufferCollectionTokenMarker>
}

/// Buffer collection info returned after allocation
#[derive(Clone, Debug, Default)]
pub struct BufferCollectionInfo {
    /// Allocated buffers
    pub buffers: Option<Vec<VmoBuffer>>,
    /// Settings used for allocation
    pub settings: Option<SingleBufferSettings>,
}

/// Single buffer in a collection
#[derive(Clone, Debug, Default)]
pub struct VmoBuffer {
    /// VMO handle (would be zx::Vmo)
    pub vmo: Option<u32>,
    /// Offset within VMO
    pub vmo_usable_start: Option<u64>,
}

/// Settings for a single buffer
#[derive(Clone, Debug, Default)]
pub struct SingleBufferSettings {
    /// Buffer settings
    pub buffer_settings: Option<BufferMemorySettings>,
    /// Image format (if applicable)
    pub image_format_constraints: Option<ImageFormatConstraints>,
}

/// Memory settings for a buffer
#[derive(Clone, Debug, Default)]
pub struct BufferMemorySettings {
    /// Size in bytes
    pub size_bytes: Option<u64>,
    /// Whether physically contiguous
    pub is_physically_contiguous: Option<bool>,
    /// Whether secure
    pub is_secure: Option<bool>,
    /// Coherency domain
    pub coherency_domain: Option<CoherencyDomain>,
    /// Heap type
    pub heap: Option<Heap>,
}

/// Marker trait for BufferCollectionToken protocol
pub struct BufferCollectionTokenMarker;

/// Marker trait for BufferCollection protocol
pub struct BufferCollectionMarker;

/// Marker trait for Allocator protocol
pub struct AllocatorMarker;
