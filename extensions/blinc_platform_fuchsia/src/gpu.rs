//! GPU integration for Fuchsia
//!
//! Provides raw window handle support for wgpu/Vulkan rendering.
//!
//! # Architecture
//!
//! On Fuchsia, GPU rendering uses:
//! - **Vulkan** via the fuchsia.vulkan.loader service
//! - **Magma** GPU driver layer (Fuchsia's GPU driver interface)
//! - **ImagePipe2** for presenting frames to Scenic/Flatland
//!
//! # Raw Window Handle
//!
//! Fuchsia doesn't use traditional window handles. Instead:
//! - wgpu creates a swapchain using Vulkan extensions
//! - Frames are presented via ImagePipe2 to Flatland
//!
//! The raw window handle abstraction is provided for compatibility.

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
}

impl FuchsiaSurfaceHandle {
    /// Create a new surface handle
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            image_pipe_token: 0, // Would be set from actual ImagePipe2 token
            width,
            height,
        }
    }

    /// Check if this handle is valid
    pub fn is_valid(&self) -> bool {
        self.image_pipe_token != 0 && self.width > 0 && self.height > 0
    }
}

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

/// Helper for creating Vulkan surface on Fuchsia
///
/// Note: This is a placeholder. Actual implementation requires:
/// 1. Creating an ImagePipe2 via Flatland
/// 2. Getting the export token
/// 3. Creating VkSurfaceKHR via VK_FUCHSIA_imagepipe_surface extension
#[cfg(target_os = "fuchsia")]
pub fn create_vulkan_surface_info(
    _handle: &FuchsiaSurfaceHandle,
) -> Result<(), blinc_platform::PlatformError> {
    // TODO: Implement actual surface creation
    // This would use:
    // - VkImagePipeSurfaceCreateInfoFUCHSIA
    // - vkCreateImagePipeSurfaceFUCHSIA
    Ok(())
}

// Re-export wgpu_types for convenience
pub use wgpu_types;

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
    fn test_gpu_config_default() {
        let config = GpuConfig::default();
        assert!(config.use_vulkan);
        assert_eq!(config.present_mode, PresentMode::Fifo);
    }
}
