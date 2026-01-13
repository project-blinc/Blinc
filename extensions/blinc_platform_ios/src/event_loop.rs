//! iOS event loop implementation
//!
//! Uses CADisplayLink for vsync-aligned rendering and RunLoop for event handling.

use crate::window::IOSWindow;
use blinc_platform::{ControlFlow, Event, EventLoop, PlatformError};

#[cfg(target_os = "ios")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "ios")]
use std::sync::Arc;

#[cfg(target_os = "ios")]
use tracing::{debug, info, warn};

/// Wake proxy for iOS event loop
///
/// Use this to request a redraw from a background animation thread.
#[cfg(target_os = "ios")]
#[derive(Clone)]
pub struct IOSWakeProxy {
    /// Flag indicating a wake was requested
    wake_requested: Arc<AtomicBool>,
}

#[cfg(target_os = "ios")]
impl IOSWakeProxy {
    /// Create a new wake proxy
    pub fn new() -> Self {
        Self {
            wake_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Wake up the event loop, causing it to process events and potentially redraw
    pub fn wake(&self) {
        self.wake_requested.store(true, Ordering::SeqCst);
        // On iOS, we rely on CADisplayLink which runs at vsync
        // The wake_requested flag will be checked on next frame
    }

    /// Check if a wake was requested and clear the flag
    pub fn take_wake_request(&self) -> bool {
        self.wake_requested.swap(false, Ordering::SeqCst)
    }
}

/// Placeholder wake proxy for non-iOS builds
#[cfg(not(target_os = "ios"))]
#[derive(Clone)]
pub struct IOSWakeProxy;

#[cfg(not(target_os = "ios"))]
impl IOSWakeProxy {
    /// Create a placeholder wake proxy
    pub fn new() -> Self {
        Self
    }

    /// No-op wake for non-iOS
    pub fn wake(&self) {}

    /// Always returns false on non-iOS
    pub fn take_wake_request(&self) -> bool {
        false
    }
}

/// iOS event loop using CADisplayLink
#[cfg(target_os = "ios")]
pub struct IOSEventLoop {
    /// Wake proxy for animation thread
    wake_proxy: IOSWakeProxy,
}

#[cfg(target_os = "ios")]
impl IOSEventLoop {
    /// Create a new iOS event loop
    pub fn new() -> Self {
        Self {
            wake_proxy: IOSWakeProxy::new(),
        }
    }

    /// Get a wake proxy that can be used to wake up the event loop from another thread
    pub fn wake_proxy(&self) -> IOSWakeProxy {
        self.wake_proxy.clone()
    }
}

#[cfg(target_os = "ios")]
impl EventLoop for IOSEventLoop {
    type Window = IOSWindow;

    fn run<F>(self, mut handler: F) -> Result<(), PlatformError>
    where
        F: FnMut(Event, &Self::Window) -> ControlFlow + 'static,
    {
        // Note: On iOS, the event loop is managed by UIApplicationMain and RunLoop
        // This implementation is a placeholder that would need to integrate with
        // the iOS application lifecycle.
        //
        // In practice, iOS apps use:
        // - UIApplicationDelegate for lifecycle events
        // - CADisplayLink for frame callbacks
        // - UIGestureRecognizers for touch events
        //
        // The actual integration happens in the IOSApp::run() in blinc_app

        info!("iOS event loop started - delegating to UIApplicationMain");

        // On iOS, we don't run a blocking event loop here
        // Instead, the run loop is managed by UIKit and we receive callbacks

        Err(PlatformError::Unsupported(
            "iOS event loop is managed by UIKit. Use IOSApp::run() from blinc_app instead."
                .to_string(),
        ))
    }
}

/// Placeholder for non-iOS builds
#[cfg(not(target_os = "ios"))]
pub struct IOSEventLoop {
    _private: (),
}

#[cfg(not(target_os = "ios"))]
impl IOSEventLoop {
    /// Create a placeholder event loop
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Get a placeholder wake proxy
    pub fn wake_proxy(&self) -> IOSWakeProxy {
        IOSWakeProxy
    }
}

#[cfg(not(target_os = "ios"))]
impl EventLoop for IOSEventLoop {
    type Window = IOSWindow;

    fn run<F>(self, _handler: F) -> Result<(), PlatformError>
    where
        F: FnMut(Event, &Self::Window) -> ControlFlow + 'static,
    {
        Err(PlatformError::Unsupported(
            "iOS platform only available on iOS".to_string(),
        ))
    }
}
