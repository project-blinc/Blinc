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
//! - [`flatland`] - Flatland 2D compositor session management
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
pub mod flatland;
pub mod gpu;
pub mod input;
pub mod scenic;
pub mod view_provider;
pub mod window;

// Re-export public types
pub use app::FuchsiaPlatform;
pub use assets::FuchsiaAssetLoader;
pub use event_loop::{
    AsyncEventLoop, EventLoopConfig, FrameScheduledEventLoop, FrameSchedulingState,
    FuchsiaEvent, FuchsiaEventLoop, FuchsiaEventSources, FuchsiaWakeProxy, InputSources,
    TouchResponse, WakeEvent,
};
pub use flatland::{
    BufferCollection, BufferFormat, ContentId, FlatlandAllocator, FlatlandError, FlatlandSession,
    HitRegion, ImageProperties, PresentArgs, SolidColor, Transform2D, TransformId,
};
pub use gpu::{
    create_fuchsia_gpu, AcquiredBuffer, FuchsiaGpu, FuchsiaSurfaceHandle, GpuConfig, GpuInfo,
    ImagePipeClient, ImagePipeError, PresentMode, PresentResult, VulkanSurface,
};
pub use input::{
    convert_key, convert_mouse, convert_touch, FocusWatcher, FuchsiaMouseButton, InputState,
    InteractionId, KeyEvent, KeyModifiers, KeyState, KeyboardListenerState, Mouse,
    MouseInteraction, MousePhase, MouseSourceState, PointerState, Touch, TouchInteraction,
    TouchPhase, TouchResponseType, TouchSourceState,
};
pub use scenic::{DisplayInfo, FocusState, FrameInfo, ScenicView, ViewProperties, ViewState};
pub use view_provider::{
    CreateView2Args, LayoutInfo, ParentViewportWatcher, ViewCreationToken, ViewEvent, ViewIdentity,
    ViewInset, ViewProvider, ViewProviderError, ViewRef,
};
pub use window::FuchsiaWindow;

// Re-export Fuchsia SDK crates for platform consumers
pub use blinc_fuchsia_zircon as zircon;
pub use blinc_fidl as fidl;
pub use blinc_fuchsia_async as fuchsia_async;

// Convenience constructor for non-Fuchsia builds
#[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
impl FuchsiaPlatform {
    /// Create a placeholder platform (for cross-compilation checks)
    pub fn with_placeholder() -> Result<Self, blinc_platform::PlatformError> {
        Err(blinc_platform::PlatformError::Unsupported(
            "Fuchsia platform only available on Fuchsia OS".to_string(),
        ))
    }
}
