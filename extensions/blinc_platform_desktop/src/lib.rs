//! Blinc Desktop Platform
//!
//! Windowing and input for macOS, Windows, and Linux using winit.
//!
//! This crate implements the `blinc_platform` traits for desktop platforms,
//! providing cross-platform windowing, input handling, and event management.
//!
//! # Example
//!
//! ```ignore
//! use blinc_platform::prelude::*;
//! use blinc_platform_desktop::DesktopPlatform;
//!
//! fn main() -> Result<(), PlatformError> {
//!     let platform = DesktopPlatform::new()?;
//!     let event_loop = platform.create_event_loop(WindowConfig::default())?;
//!
//!     event_loop.run(|event, window| {
//!         match event {
//!             Event::Frame => {
//!                 // Render frame here
//!             }
//!             Event::Window(WindowEvent::CloseRequested) => {
//!                 return ControlFlow::Exit;
//!             }
//!             _ => {}
//!         }
//!         ControlFlow::Continue
//!     })
//! }
//! ```

pub mod event_loop;
pub mod input;
pub mod window;

pub use event_loop::DesktopEventLoop;
pub use window::DesktopWindow;

use blinc_platform::{Platform, PlatformError, WindowConfig};

/// Desktop platform implementation
///
/// Provides windowing and input for macOS, Windows, and Linux.
pub struct DesktopPlatform;

impl Platform for DesktopPlatform {
    type Window = DesktopWindow;
    type EventLoop = DesktopEventLoop;

    fn new() -> Result<Self, PlatformError> {
        Ok(Self)
    }

    fn create_event_loop(&self) -> Result<Self::EventLoop, PlatformError> {
        DesktopEventLoop::new(WindowConfig::default())
    }

    fn name(&self) -> &'static str {
        "desktop"
    }

    fn scale_factor(&self) -> f64 {
        // Default scale factor; actual value comes from window
        1.0
    }
}

impl DesktopPlatform {
    /// Create an event loop with custom window configuration
    pub fn create_event_loop_with_config(
        &self,
        config: WindowConfig,
    ) -> Result<DesktopEventLoop, PlatformError> {
        DesktopEventLoop::new(config)
    }
}
