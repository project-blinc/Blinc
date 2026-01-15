//! Blinc Fuchsia Platform
//!
//! Scenic compositor integration and Vulkan rendering for Fuchsia OS.
//!
//! This crate implements the `blinc_platform` traits for Fuchsia,
//! providing touch/mouse input, lifecycle management, and window handling
//! via Scenic and FIDL.
//!
//! # Architecture
//!
//! Fuchsia uses a component-based architecture where Blinc integrates through:
//!
//! - **Scenic/Flatland** for window compositing via Views
//! - **fuchsia-async** for async event handling
//! - **FIDL** for IPC with system services
//! - **Vulkan** for GPU rendering via ImagePipe2
//!
//! # Modules
//!
//! - [`app`] - Platform trait implementation
//! - [`window`] - Window/View management
//! - [`event_loop`] - Event handling and frame scheduling
//! - [`input`] - Touch, mouse, and keyboard input
//! - [`scenic`] - Scenic compositor integration types
//! - [`gpu`] - GPU/Vulkan integration helpers
//! - [`assets`] - Asset loading from package namespace
//!
//! # Usage
//!
//! ```ignore
//! use blinc_app::fuchsia::FuchsiaApp;
//!
//! fn main() {
//!     FuchsiaApp::run(|ctx| {
//!         div()
//!             .w(ctx.width).h(ctx.height)
//!             .bg([0.1, 0.1, 0.15, 1.0])
//!             .flex_center()
//!             .child(text("Hello Fuchsia!").size(48.0))
//!     }).unwrap();
//! }
//! ```
//!
//! # Building for Fuchsia
//!
//! Requires the Fuchsia SDK and appropriate Rust targets:
//!
//! ```bash
//! # Install SDK
//! ./scripts/setup-fuchsia-sdk.sh
//!
//! # Add Rust targets
//! rustup target add x86_64-unknown-fuchsia
//! rustup target add aarch64-unknown-fuchsia
//!
//! # Build
//! cargo build --target x86_64-unknown-fuchsia --features fuchsia
//! ```
//!
//! # Component Manifest
//!
//! Fuchsia apps require a component manifest (.cml). See [`scenic::manifest`]
//! for helpers and examples.
//!
//! # References
//!
//! - [Fuchsia SDK](https://fuchsia.dev/fuchsia-src/development/sdk)
//! - [Scenic Overview](https://fuchsia.dev/fuchsia-src/concepts/graphics/scenic)
//! - [Flatland Guide](https://fuchsia.dev/fuchsia-src/development/graphics/flatland)
//! - [Input Protocol](https://fuchsia.dev/fuchsia-src/concepts/ui/input)

pub mod app;
pub mod assets;
pub mod event_loop;
pub mod gpu;
pub mod input;
pub mod scenic;
pub mod window;

// Re-export public types
pub use app::FuchsiaPlatform;
pub use assets::FuchsiaAssetLoader;
pub use event_loop::{FuchsiaEventLoop, FuchsiaWakeProxy};
pub use gpu::{FuchsiaSurfaceHandle, GpuConfig, GpuInfo, PresentMode};
pub use input::{
    convert_key, convert_mouse, convert_touch, FuchsiaMouseButton, KeyEvent, KeyModifiers,
    KeyState, Mouse, MousePhase, Touch, TouchPhase,
};
pub use scenic::{DisplayInfo, FocusState, FrameInfo, ScenicView, ViewProperties, ViewState};
pub use window::FuchsiaWindow;

use blinc_platform::PlatformError;

// Convenience constructor for non-Fuchsia builds
#[cfg(not(target_os = "fuchsia"))]
impl FuchsiaPlatform {
    /// Create a placeholder platform (for cross-compilation checks)
    pub fn with_placeholder() -> Result<Self, PlatformError> {
        Err(PlatformError::Unsupported(
            "Fuchsia platform only available on Fuchsia OS".to_string(),
        ))
    }
}
