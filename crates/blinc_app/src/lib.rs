//! Blinc Application Framework
//!
//! Clean API for building Blinc applications with layout and rendering.
//!
//! # Example (Headless Rendering)
//!
//! ```ignore
//! use blinc_app::prelude::*;
//!
//! fn main() -> Result<()> {
//!     let app = BlincApp::new()?;
//!
//!     let ui = div()
//!         .w(400.0).h(300.0)
//!         .flex_col().gap(4.0).p(4.0)
//!         .child(
//!             div().glass()
//!                 .w_full().h(100.0)
//!                 .rounded(16.0)
//!                 .child(text("Hello Blinc!").size(24.0))
//!         );
//!
//!     app.render(&ui, &target_view, 400.0, 300.0)?;
//! }
//! ```
//!
//! # Example (Windowed Application)
//!
//! ```ignore
//! use blinc_app::prelude::*;
//! use blinc_app::windowed::{WindowedApp, WindowedContext};
//!
//! fn main() -> Result<()> {
//!     WindowedApp::run(WindowConfig::default(), |ctx| {
//!         div()
//!             .w(ctx.width).h(ctx.height)
//!             .bg([0.1, 0.1, 0.15, 1.0])
//!             .flex_center()
//!             .child(
//!                 div().glass().rounded(16.0).p(24.0)
//!                     .child(text("Hello Blinc!").size(32.0))
//!             )
//!     })
//! }
//! ```

/// Get the paths to system default fonts, in priority order.
///
/// Returns a list of font paths to try loading, with the best choice first.
/// - macOS: San Francisco (SFNS.ttf) first, then Helvetica
/// - Linux: DejaVu Sans
/// - Windows: Segoe UI
pub fn system_font_paths() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    {
        &[
            "/System/Library/Fonts/SFNS.ttf", // San Francisco - primary system font
            "/System/Library/Fonts/Helvetica.ttc", // Fallback
        ]
    }
    #[cfg(target_os = "linux")]
    {
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
        ]
    }
    #[cfg(target_os = "windows")]
    {
        &["C:\\Windows\\Fonts\\segoeui.ttf"]
    }
    #[cfg(target_os = "android")]
    {
        &[
            "/system/fonts/Roboto-Regular.ttf",
            "/system/fonts/NotoSansCJK-Regular.ttc",
            "/system/fonts/DroidSans.ttf",
        ]
    }
    #[cfg(target_os = "ios")]
    {
        // iOS system fonts - Core directory is most reliable
        &[
            "/System/Library/Fonts/Core/SFUI.ttf",          // SF UI (system font)
            "/System/Library/Fonts/Core/SFUIMono.ttf",      // SF Mono
            "/System/Library/Fonts/Core/Helvetica.ttc",     // Helvetica
            "/System/Library/Fonts/Core/HelveticaNeue.ttc", // Helvetica Neue
            "/System/Library/Fonts/Core/Avenir.ttc",        // Avenir
            "/System/Library/Fonts/CoreUI/Menlo.ttc",       // Menlo (monospace)
        ]
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "windows",
        target_os = "android",
        target_os = "ios"
    )))]
    {
        &[]
    }
}

mod app;
mod context;
mod error;
mod text_measurer;

// Windowed module is compiled for desktop (windowed feature), Android, and iOS
// since WindowedContext and shared types are used by all platforms
#[cfg(any(
    feature = "windowed",
    all(feature = "android", target_os = "android"),
    all(feature = "ios", target_os = "ios")
))]
pub mod windowed;

#[cfg(all(feature = "android", target_os = "android"))]
pub mod android;
#[cfg(all(feature = "android", target_os = "android"))]
pub use android::AndroidApp;

#[cfg(all(feature = "ios", target_os = "ios"))]
pub mod ios;

#[cfg(test)]
mod tests;

pub use app::{BlincApp, BlincConfig};
pub use context::{DebugMode, RenderContext};
pub use error::{BlincError, Result};
pub use text_measurer::{init_text_measurer, init_text_measurer_with_registry, FontTextMeasurer};

// Re-export layout API for convenience
pub use blinc_layout::prelude::*;
pub use blinc_layout::RenderTree;

// Re-export platform types for windowed applications
pub use blinc_platform::WindowConfig;

// Re-export derive macro
pub use blinc_macros::BlincComponent;

/// Prelude module - import everything commonly needed
pub mod prelude {
    pub use crate::app::{BlincApp, BlincConfig};
    pub use crate::context::{DebugMode, RenderContext};
    pub use crate::error::{BlincError, Result};
    pub use crate::text_measurer::{init_text_measurer, init_text_measurer_with_registry};

    // Layout builders
    pub use blinc_layout::prelude::*;
    pub use blinc_layout::RenderTree;

    // Core types
    pub use blinc_core::{Color, Point, Rect, Size};

    // Reactive primitives
    pub use blinc_core::reactive::{Derived, Effect, ReactiveGraph, Signal};

    // Platform types
    pub use blinc_platform::WindowConfig;

    // Derive macro for components
    pub use blinc_macros::BlincComponent;

    // Theme types
    pub use blinc_theme::{ColorScheme, ColorToken, RadiusToken, SpacingToken, ThemeState};
}
