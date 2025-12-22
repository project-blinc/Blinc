//! Event loop and platform events

use crate::error::PlatformError;
use crate::input::InputEvent;
use crate::window::Window;

/// Event loop abstraction
///
/// Platform-specific event loops implement this trait to provide
/// a unified interface for running Blinc applications.
pub trait EventLoop {
    /// The window type for this event loop
    type Window: Window;

    /// Run the event loop
    ///
    /// This method blocks until the application exits. The handler
    /// function is called for each event, and should return a
    /// `ControlFlow` to indicate whether to continue or exit.
    fn run<F>(self, handler: F) -> Result<(), PlatformError>
    where
        F: FnMut(Event, &Self::Window) -> ControlFlow + 'static;
}

/// Control flow after handling an event
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlFlow {
    /// Continue running the event loop
    #[default]
    Continue,
    /// Exit the event loop
    Exit,
}

/// Platform events
#[derive(Clone, Debug)]
pub enum Event {
    /// Window-related event
    Window(WindowEvent),
    /// Input event (mouse, keyboard, touch)
    Input(InputEvent),
    /// Application lifecycle event
    Lifecycle(LifecycleEvent),
    /// Frame tick - time to render
    ///
    /// This event is sent when the application should render a frame.
    /// On desktop, this typically happens after vsync or at 60fps.
    /// On mobile, this happens when the app is focused and ready.
    Frame,
}

/// Window events
#[derive(Clone, Debug)]
pub enum WindowEvent {
    /// Window was resized
    Resized {
        /// New width in physical pixels
        width: u32,
        /// New height in physical pixels
        height: u32,
    },
    /// Window was moved
    Moved {
        /// New X position
        x: i32,
        /// New Y position
        y: i32,
    },
    /// Window close was requested (e.g., close button clicked)
    CloseRequested,
    /// Window gained or lost focus
    Focused(bool),
    /// Display scale factor changed
    ScaleFactorChanged {
        /// New scale factor
        scale_factor: f64,
    },
}

/// Application lifecycle events
#[derive(Clone, Debug)]
pub enum LifecycleEvent {
    /// Application resumed (came to foreground)
    Resumed,
    /// Application suspended (went to background)
    Suspended,
    /// System is low on memory - release caches if possible
    LowMemory,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_flow_default() {
        assert_eq!(ControlFlow::default(), ControlFlow::Continue);
    }
}
