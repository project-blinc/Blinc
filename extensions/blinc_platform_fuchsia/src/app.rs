//! Fuchsia platform implementation
//!
//! Implements the Platform trait for Fuchsia OS using Scenic compositor.

use blinc_platform::{Platform, PlatformError};

use crate::event_loop::FuchsiaEventLoop;
use crate::window::FuchsiaWindow;

/// Fuchsia platform using Scenic compositor
pub struct FuchsiaPlatform {
    /// Display scale factor
    #[allow(dead_code)] // Used in #[cfg(target_os = "fuchsia")]
    scale_factor: f64,
}

impl FuchsiaPlatform {
    /// Get system font paths for Fuchsia
    pub fn system_font_paths() -> &'static [&'static str] {
        &[
            "/pkg/data/fonts/Roboto-Regular.ttf",
            "/system/fonts/Roboto-Regular.ttf",
        ]
    }
}

#[cfg(target_os = "fuchsia")]
impl Platform for FuchsiaPlatform {
    type Window = FuchsiaWindow;
    type EventLoop = FuchsiaEventLoop;

    fn new() -> Result<Self, PlatformError> {
        // TODO: Connect to Scenic via FIDL
        // TODO: Get display metrics from fuchsia.ui.display
        Ok(Self { scale_factor: 1.0 })
    }

    fn name(&self) -> &'static str {
        "fuchsia"
    }

    fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    fn create_event_loop(&self) -> Result<Self::EventLoop, PlatformError> {
        Ok(FuchsiaEventLoop::new())
    }
}

// Placeholder for non-Fuchsia builds
#[cfg(not(target_os = "fuchsia"))]
impl Platform for FuchsiaPlatform {
    type Window = FuchsiaWindow;
    type EventLoop = FuchsiaEventLoop;

    fn new() -> Result<Self, PlatformError> {
        Err(PlatformError::Unsupported(
            "Fuchsia platform only available on Fuchsia OS".to_string(),
        ))
    }

    fn name(&self) -> &'static str {
        "fuchsia"
    }

    fn scale_factor(&self) -> f64 {
        1.0
    }

    fn create_event_loop(&self) -> Result<Self::EventLoop, PlatformError> {
        Err(PlatformError::Unsupported(
            "Fuchsia platform only available on Fuchsia OS".to_string(),
        ))
    }
}
