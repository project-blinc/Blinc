//! Blinc Platform Abstraction Layer
//!
//! This crate provides platform-agnostic traits and types for windowing,
//! input handling, and application lifecycle management.
//!
//! # Architecture
//!
//! The platform abstraction is built around three main traits:
//!
//! - [`Platform`] - The top-level platform abstraction
//! - [`Window`] - Window management and properties
//! - [`EventLoop`] - Event handling and application lifecycle
//!
//! # Platform Implementations
//!
//! - `blinc_platform_desktop` - Desktop platforms (macOS, Windows, Linux) using winit
//! - `blinc_platform_android` - Android using NDK
//! - `blinc_platform_ios` - iOS using UIKit (planned)
//!
//! # Example
//!
//! ```ignore
//! use blinc_platform::*;
//! use blinc_platform_desktop::DesktopPlatform;
//!
//! fn main() -> Result<(), PlatformError> {
//!     let platform = DesktopPlatform::new()?;
//!     let event_loop = platform.create_event_loop()?;
//!
//!     event_loop.run(|event, window| {
//!         match event {
//!             Event::Frame => {
//!                 // Render frame
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

mod error;
mod event;
mod input;
mod platform;
mod window;

// Re-export all public types
pub use error::{PlatformError, Result};
pub use event::{ControlFlow, Event, EventLoop, LifecycleEvent, WindowEvent};
pub use input::{
    InputEvent, Key, KeyState, KeyboardEvent, Modifiers, MouseButton, MouseEvent, TouchEvent,
};
pub use platform::Platform;
pub use window::{Cursor, Window, WindowConfig};

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::error::{PlatformError, Result};
    pub use crate::event::{ControlFlow, Event, EventLoop, LifecycleEvent, WindowEvent};
    pub use crate::input::{
        InputEvent, Key, KeyState, KeyboardEvent, Modifiers, MouseButton, MouseEvent, TouchEvent,
    };
    pub use crate::platform::Platform;
    pub use crate::window::{Cursor, Window, WindowConfig};
}
