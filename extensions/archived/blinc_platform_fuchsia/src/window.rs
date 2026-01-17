//! Fuchsia window implementation
//!
//! Wraps a Scenic View for window management.

use blinc_platform::{Cursor, Window};

/// Fuchsia window backed by a Scenic View
pub struct FuchsiaWindow {
    /// Window width in logical pixels
    width: u32,
    /// Window height in logical pixels
    height: u32,
    /// Display scale factor
    scale_factor: f64,
}

impl FuchsiaWindow {
    /// Create a new Fuchsia window
    pub fn new(scale_factor: f64) -> Self {
        Self {
            width: 1920,
            height: 1080,
            scale_factor,
        }
    }

    /// Update window size from ViewProperties
    #[cfg(target_os = "fuchsia")]
    pub fn update_size(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }
}

impl Window for FuchsiaWindow {
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn logical_size(&self) -> (f32, f32) {
        (
            self.width as f32 / self.scale_factor as f32,
            self.height as f32 / self.scale_factor as f32,
        )
    }

    fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    fn set_title(&self, _title: &str) {
        // Fuchsia apps don't have traditional window titles
        // The app name comes from the component manifest
    }

    fn set_cursor(&self, _cursor: Cursor) {
        // Fuchsia is primarily touch-based; cursor support is limited
        // TODO: Implement for desktop Fuchsia devices
    }

    fn request_redraw(&self) {
        // TODO: Signal Scenic to schedule next frame
    }

    fn is_focused(&self) -> bool {
        // TODO: Track focus state from Scenic view events
        true
    }

    fn is_visible(&self) -> bool {
        // TODO: Track visibility from Scenic
        true
    }
}
