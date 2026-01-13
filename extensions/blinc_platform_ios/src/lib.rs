//! Blinc iOS Platform
//!
//! UIKit integration and Metal rendering for iOS.
//!
//! This crate implements the `blinc_platform` traits for iOS,
//! providing touch input, lifecycle management, and window handling
//! via UIKit and Metal.
//!
//! # Architecture
//!
//! Unlike desktop platforms where Blinc owns the event loop, on iOS
//! the event loop is managed by UIKit's RunLoop. Blinc integrates with
//! iOS through:
//!
//! - **CADisplayLink** for vsync-aligned frame callbacks
//! - **UIGestureRecognizers** for touch input
//! - **UIApplicationDelegate** for lifecycle events
//! - **CAMetalLayer** for GPU rendering with Metal
//!
//! # Usage
//!
//! ```ignore
//! use blinc_app::ios::IOSApp;
//!
//! // In your app delegate's application:didFinishLaunchingWithOptions:
//! IOSApp::run(metal_layer, |ctx| {
//!     div()
//!         .w(ctx.width).h(ctx.height)
//!         .bg([0.1, 0.1, 0.15, 1.0])
//!         .flex_center()
//!         .child(text("Hello iOS!").size(48.0))
//! })
//! ```

pub mod app;
pub mod assets;
pub mod event_loop;
pub mod input;
pub mod window;

// Re-export public types
pub use app::{
    get_display_scale, get_safe_area_insets, is_dark_mode, system_font_paths, IOSPlatform,
};
pub use assets::IOSAssetLoader;
pub use event_loop::{IOSEventLoop, IOSWakeProxy};
pub use input::{convert_touch, convert_touches, Gesture, GestureDetector, Touch, TouchPhase};
pub use window::IOSWindow;

// iOS-specific entry point
#[cfg(target_os = "ios")]
pub use app::ios_main;

use blinc_platform::PlatformError;

// Convenience constructor for non-iOS builds
#[cfg(not(target_os = "ios"))]
impl IOSPlatform {
    /// Create a placeholder platform (for cross-compilation checks)
    pub fn with_placeholder() -> Result<Self, PlatformError> {
        Err(PlatformError::Unsupported(
            "iOS platform only available on iOS".to_string(),
        ))
    }
}
