//! iOS window implementation
//!
//! Wraps UIWindow and provides a CAMetalLayer for GPU rendering.

use blinc_platform::{Cursor, Window};

#[cfg(target_os = "ios")]
use objc2::rc::Retained;

#[cfg(target_os = "ios")]
use objc2_metal::MTLDevice;

#[cfg(target_os = "ios")]
use objc2_ui_kit::UIWindow;

#[cfg(target_os = "ios")]
use std::cell::Cell;

/// iOS window wrapping UIWindow with Metal layer
#[cfg(target_os = "ios")]
pub struct IOSWindow {
    /// The UIWindow object
    window: Retained<UIWindow>,
    /// Scale factor (contentScaleFactor)
    scale_factor: f64,
    /// Physical width in pixels
    width: Cell<u32>,
    /// Physical height in pixels
    height: Cell<u32>,
    /// Whether the window is focused
    focused: Cell<bool>,
    /// Whether the window is visible
    visible: Cell<bool>,
}

#[cfg(target_os = "ios")]
impl IOSWindow {
    /// Create a new iOS window
    pub fn new(window: Retained<UIWindow>, scale_factor: f64, width: u32, height: u32) -> Self {
        Self {
            window,
            scale_factor,
            width: Cell::new(width),
            height: Cell::new(height),
            focused: Cell::new(true),
            visible: Cell::new(true),
        }
    }

    /// Get the underlying UIWindow
    pub fn ui_window(&self) -> &UIWindow {
        &self.window
    }

    /// Update the window size
    pub fn set_size(&self, width: u32, height: u32) {
        self.width.set(width);
        self.height.set(height);
    }

    /// Set focused state
    pub fn set_focused(&self, focused: bool) {
        self.focused.set(focused);
    }

    /// Set visible state
    pub fn set_visible(&self, visible: bool) {
        self.visible.set(visible);
    }
}

#[cfg(target_os = "ios")]
impl Window for IOSWindow {
    fn size(&self) -> (u32, u32) {
        (self.width.get(), self.height.get())
    }

    fn logical_size(&self) -> (f32, f32) {
        let (w, h) = self.size();
        (
            w as f32 / self.scale_factor as f32,
            h as f32 / self.scale_factor as f32,
        )
    }

    fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    fn set_title(&self, _title: &str) {
        // iOS doesn't have window titles in the traditional sense
    }

    fn set_cursor(&self, _cursor: Cursor) {
        // iOS doesn't have cursors - all interaction is via touch
    }

    fn request_redraw(&self) {
        // Request redraw is handled by CADisplayLink
        // The display link will call our render callback on next vsync
    }

    fn is_focused(&self) -> bool {
        self.focused.get()
    }

    fn is_visible(&self) -> bool {
        self.visible.get()
    }
}

// Send is safe because UIWindow is only accessed from main thread
#[cfg(target_os = "ios")]
unsafe impl Send for IOSWindow {}

/// Placeholder for non-iOS builds
#[cfg(not(target_os = "ios"))]
pub struct IOSWindow {
    _private: (),
}

#[cfg(not(target_os = "ios"))]
impl IOSWindow {
    /// Create a placeholder window (for cross-compilation checks)
    pub fn new() -> Self {
        Self { _private: () }
    }
}

#[cfg(not(target_os = "ios"))]
impl Window for IOSWindow {
    fn size(&self) -> (u32, u32) {
        (0, 0)
    }

    fn logical_size(&self) -> (f32, f32) {
        (0.0, 0.0)
    }

    fn scale_factor(&self) -> f64 {
        1.0
    }

    fn set_title(&self, _title: &str) {}

    fn set_cursor(&self, _cursor: Cursor) {}

    fn request_redraw(&self) {}

    fn is_focused(&self) -> bool {
        false
    }

    fn is_visible(&self) -> bool {
        false
    }
}
