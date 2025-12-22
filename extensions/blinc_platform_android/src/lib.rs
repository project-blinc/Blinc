//! Blinc Android Platform
//!
//! Android platform implementation for Blinc UI framework.
//!
//! This crate implements the `blinc_platform` traits for Android,
//! providing touch input, lifecycle management, and window handling
//! via the Android NDK.
//!
//! # Example
//!
//! ```ignore
//! use blinc_platform::prelude::*;
//! use blinc_platform_android::AndroidPlatform;
//!
//! // AndroidPlatform is created internally by the android_main entry point
//! // Use the event loop to handle events:
//! event_loop.run(|event, window| {
//!     match event {
//!         Event::Frame => {
//!             // Render frame
//!         }
//!         Event::Lifecycle(LifecycleEvent::Suspended) => {
//!             // App going to background
//!         }
//!         _ => {}
//!     }
//!     ControlFlow::Continue
//! })
//! ```

pub mod activity;
pub mod event_loop;
pub mod input;
pub mod window;

pub use event_loop::AndroidEventLoop;
pub use window::AndroidWindow;

use blinc_platform::{Platform, PlatformError};

/// Android platform implementation
///
/// Provides touch input and lifecycle management for Android apps.
pub struct AndroidPlatform {
    #[cfg(target_os = "android")]
    app: android_activity::AndroidApp,
}

#[cfg(target_os = "android")]
impl AndroidPlatform {
    /// Create a new Android platform with the given AndroidApp
    pub fn with_app(app: android_activity::AndroidApp) -> Result<Self, PlatformError> {
        Ok(Self { app })
    }
}

impl Platform for AndroidPlatform {
    type Window = AndroidWindow;
    type EventLoop = AndroidEventLoop;

    fn new() -> Result<Self, PlatformError> {
        // On Android, the platform must be created with an AndroidApp from android_main
        Err(PlatformError::InitFailed(
            "AndroidPlatform must be created with AndroidPlatform::with_app()".to_string(),
        ))
    }

    #[cfg(target_os = "android")]
    fn create_event_loop(&self) -> Result<Self::EventLoop, PlatformError> {
        Ok(AndroidEventLoop::new(self.app.clone()))
    }

    #[cfg(not(target_os = "android"))]
    fn create_event_loop(&self) -> Result<Self::EventLoop, PlatformError> {
        Err(PlatformError::Unsupported(
            "Android platform only available on Android".to_string(),
        ))
    }

    fn name(&self) -> &'static str {
        "android"
    }

    fn scale_factor(&self) -> f64 {
        // TODO: Get actual density from DisplayMetrics via JNI
        1.0
    }
}

// Placeholder implementation for non-Android builds
#[cfg(not(target_os = "android"))]
impl AndroidPlatform {
    /// Create a placeholder platform (for cross-compilation checks)
    pub fn with_app() -> Result<Self, PlatformError> {
        Err(PlatformError::Unsupported(
            "Android platform only available on Android".to_string(),
        ))
    }
}

// Android-specific entry point
#[cfg(target_os = "android")]
pub use activity::android_main;

/// Input conversion utilities
pub mod input_convert {
    use super::input::{TouchEvent, TouchPointer};
    use blinc_platform::{InputEvent, TouchEvent as BlincTouchEvent};

    /// Convert Android TouchEvent to blinc_platform InputEvent
    pub fn convert_touch_event(event: &TouchEvent) -> InputEvent {
        match event {
            TouchEvent::Down { pointer, .. } => InputEvent::Touch(BlincTouchEvent::Started {
                id: pointer.id as u64,
                x: pointer.x,
                y: pointer.y,
                pressure: pointer.pressure,
            }),
            TouchEvent::Move { pointers } => {
                // Report first pointer for move events
                if let Some(p) = pointers.first() {
                    InputEvent::Touch(BlincTouchEvent::Moved {
                        id: p.id as u64,
                        x: p.x,
                        y: p.y,
                        pressure: p.pressure,
                    })
                } else {
                    InputEvent::Touch(BlincTouchEvent::Cancelled { id: 0 })
                }
            }
            TouchEvent::Up { pointer, .. } => InputEvent::Touch(BlincTouchEvent::Ended {
                id: pointer.id as u64,
                x: pointer.x,
                y: pointer.y,
            }),
            TouchEvent::Cancel => InputEvent::Touch(BlincTouchEvent::Cancelled { id: 0 }),
        }
    }

    /// Convert all touch pointers from a multi-touch move event
    pub fn convert_multi_touch_move(pointers: &[TouchPointer]) -> Vec<InputEvent> {
        pointers
            .iter()
            .map(|p| {
                InputEvent::Touch(BlincTouchEvent::Moved {
                    id: p.id as u64,
                    x: p.x,
                    y: p.y,
                    pressure: p.pressure,
                })
            })
            .collect()
    }
}

// Re-export gesture detection
pub use input::GestureDetector;
