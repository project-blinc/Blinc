#![allow(unused, dead_code, deprecated)]
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
    // Linux (but not OHOS which also reports target_os = "linux")
    #[cfg(all(target_os = "linux", not(target_env = "ohos")))]
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
            "/System/Library/Fonts/Core/SFUI.ttf", // SF UI (system font)
            "/System/Library/Fonts/Core/SFUIMono.ttf", // SF Mono
            "/System/Library/Fonts/Core/Helvetica.ttc", // Helvetica
            "/System/Library/Fonts/Core/HelveticaNeue.ttc", // Helvetica Neue
            "/System/Library/Fonts/Core/Avenir.ttc", // Avenir
            "/System/Library/Fonts/CoreUI/Menlo.ttc", // Menlo (monospace)
        ]
    }
    #[cfg(target_os = "fuchsia")]
    {
        // Fuchsia system fonts - from package namespace or system fonts
        &[
            "/pkg/data/fonts/Roboto-Regular.ttf",
            "/system/fonts/Roboto-Regular.ttf",
        ]
    }
    #[cfg(target_env = "ohos")]
    {
        // HarmonyOS/OpenHarmony system fonts
        &[
            "/system/fonts/HarmonyOS_Sans_SC_Regular.ttf",
            "/system/fonts/Roboto-Regular.ttf",
            "/system/fonts/NotoSansCJK-Regular.ttc",
        ]
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "windows",
        target_os = "android",
        target_os = "ios",
        target_os = "fuchsia",
        target_env = "ohos"
    )))]
    {
        &[]
    }
}

mod app;
mod context;
mod error;
mod svg_atlas;

/// Subsecond hot-reload websocket client.
///
/// Compiled only when the `hot-reload` feature is on. Spawns a thread
/// that connects to `dx serve --hot-patch`'s dev-server and applies
/// incoming jump-table patches. See module docs for the wire format
/// and dep-tree rationale.
#[cfg(feature = "hot-reload")]
pub mod hot_reload;

// Windowed module is compiled for desktop (windowed feature), Android, iOS, Fuchsia, HarmonyOS,
// AND web — since `WindowedContext` and the shared scheduler / overlay / registry types are
// used by every platform runner. The web runner also lives behind a wasm32 cfg gate (see below).
#[cfg(any(
    feature = "windowed",
    all(feature = "android", target_os = "android"),
    all(feature = "ios", target_os = "ios"),
    all(feature = "fuchsia", target_os = "fuchsia"),
    all(feature = "harmony", target_env = "ohos"),
    all(feature = "web", target_arch = "wasm32")
))]
pub mod windowed;

/// EXPERIMENTAL — hand-rolled Wayland frame-callback gate. Mirrors
/// GPUI's approach on Linux: register `wl_surface::frame()` callbacks
/// directly via wayland-client and gate `Surface::get_current_texture()`
/// on `Done` ourselves, bypassing winit's `pre_present_notify` chain
/// when that chain doesn't engage on some Mesa + Wayland compositors.
/// Opt-in via the `wayland-frame-gate` feature; module is a no-op
/// elsewhere.
#[cfg(all(feature = "wayland-frame-gate", target_os = "linux"))]
pub mod wayland_frame_gate;

/// Native file dialogs (open, save, folder picker).
/// Available on desktop when the `windowed` feature is enabled.
#[cfg(feature = "rfd")]
pub mod dialog;

/// Cross-platform callback-based file open (`open_file_into`) — works
/// on desktop (rfd) AND web (`<input type=file>`), unlike the
/// synchronous desktop-only `dialog` API (the `dialogs`/`rfd` feature).
pub mod file_open;

/// Window state persistence (save/restore position, size, maximized).
pub mod window_state;

/// System tray icon support (desktop only).
pub mod tray;

/// Native OS desktop notifications.
pub mod notify;

/// Global keyboard shortcuts.
pub mod hotkey;

/// Drag and drop support.
pub mod dnd;

#[cfg(all(feature = "android", target_os = "android"))]
pub mod android;
#[cfg(all(feature = "android", target_os = "android"))]
pub use android::AndroidApp;

#[cfg(all(feature = "ios", target_os = "ios"))]
pub mod ios;

#[cfg(all(feature = "fuchsia", target_os = "fuchsia"))]
pub mod fuchsia;
#[cfg(all(feature = "fuchsia", target_os = "fuchsia"))]
pub use fuchsia::FuchsiaApp;

#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub mod web;
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub use web::WebApp;

#[cfg(test)]
mod tests;

pub use app::{BlincApp, BlincConfig};
pub use context::{DebugMode, RenderContext};
pub use error::{BlincError, Result};
// The measurer moved to blinc_layout so a host driving a LayoutTree
// without blinc_app can still measure with real fonts. Re-exported
// here under its original names.
pub use blinc_layout::{FontTextMeasurer, init_text_measurer, init_text_measurer_with_registry};

/// Register a font face into the process-wide `blinc_text` font
/// registry. The returned count is the number of faces fontdb
/// parsed from the bytes — typically `1` for a plain `.ttf` / `.otf`
/// and higher for a `.ttc` collection. Callable before
/// `WindowedApp::run` (or any other runner) so the UI can depend on
/// the font being available from the very first frame.
///
/// Thin wrapper over `blinc_text::global_font_registry().lock()`.
/// `BlincApp` / `WindowedContext` / `TextRenderer::new()` all back
/// themselves with the same shared registry, so a face registered
/// here is immediately visible to every text renderer the process
/// spins up — no further plumbing required.
///
/// # Example
///
/// ```ignore
/// fn main() -> blinc_app::Result<()> {
///     blinc_app::register_font(include_bytes!("assets/Inter.ttf").to_vec());
///     blinc_app::windowed::WindowedApp::run(config, build_ui)
/// }
/// ```
pub fn register_font(data: Vec<u8>) -> usize {
    match blinc_text::global_font_registry().lock() {
        Ok(mut reg) => reg.load_font_data(data),
        Err(_) => 0,
    }
}

// Re-export layout API for convenience
pub use blinc_layout::RenderTree;
pub use blinc_layout::prelude::*;

// Re-export platform types for windowed applications
pub use blinc_platform::{AnimationThreadMode, WindowConfig, WindowLevel};

// Re-export derive macro
pub use blinc_macros::BlincComponent;
/// Short alias for the [`BlincComponent`] derive macro.
pub use blinc_macros::BlincComponent as Component;

/// Short alias for [`blinc_core::BlincContextState`] — the global state singleton.
pub use blinc_core::BlincContextState as Context;

/// Prelude module - import everything commonly needed
pub mod prelude {
    pub use crate::app::{BlincApp, BlincConfig};
    pub use crate::context::{DebugMode, RenderContext};
    pub use crate::error::{BlincError, Result};
    pub use crate::register_font;
    pub use blinc_layout::{init_text_measurer, init_text_measurer_with_registry};

    // Layout builders
    pub use blinc_layout::RenderTree;
    pub use blinc_layout::prelude::*;

    // Core types
    pub use blinc_core::{Color, Point, Rect, Size};

    // Reactive primitives
    pub use blinc_core::reactive::{Derived, Effect, ReactiveGraph, Signal};

    // Platform types
    pub use blinc_platform::{AnimationThreadMode, WindowConfig, WindowLevel};

    // Derive macro for components
    pub use blinc_macros::BlincComponent;
    /// Short alias for the [`BlincComponent`] derive macro.
    pub use blinc_macros::BlincComponent as Component;

    /// Short alias for `BlincContextState` — the global state singleton.
    pub use blinc_core::BlincContextState as Context;

    // Theme types
    pub use blinc_theme::{
        ColorScheme, ColorToken, RadiusToken, ShapeToken, ShapeTokens, SpacingToken, ThemeBundle,
        ThemeState,
    };

    /// Universal HID themes. Hybrid is the recommended cross-platform
    /// default; Restrained and Expressive sit on either side of the
    /// restraint↔expressiveness axis.
    pub use blinc_theme::{DefaultTheme, ExpressiveTheme, HybridTheme, RestrainedTheme};
}
