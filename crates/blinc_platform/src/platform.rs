//! Platform trait and abstraction

use crate::error::PlatformError;
use crate::event::EventLoop;
use crate::window::Window;

/// Platform abstraction trait
///
/// This trait is implemented by each platform backend (desktop, android, ios)
/// to provide a unified interface for creating windows and running event loops.
pub trait Platform: Send + Sync {
    /// The window type for this platform
    type Window: Window;
    /// The event loop type for this platform
    type EventLoop: EventLoop<Window = Self::Window>;

    /// Create a new platform instance
    fn new() -> Result<Self, PlatformError>
    where
        Self: Sized;

    /// Create an event loop
    ///
    /// The event loop is used to receive platform events and drive
    /// the application's main loop.
    fn create_event_loop(&self) -> Result<Self::EventLoop, PlatformError>;

    /// Get the platform name
    ///
    /// Returns a string like "desktop", "android", or "ios".
    fn name(&self) -> &'static str;

    /// Get the default display scale factor
    ///
    /// This returns the system's default scale factor for DPI scaling.
    /// Individual windows may have different scale factors.
    fn scale_factor(&self) -> f64;
}
