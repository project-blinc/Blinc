//! Android window implementation
//!
//! Wraps an Android NativeWindow to implement the blinc_platform Window trait.

use blinc_platform::{Cursor, Window};
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "android")]
use ndk::native_window::NativeWindow;

/// Android window wrapping an NDK NativeWindow
#[cfg(target_os = "android")]
pub struct AndroidWindow {
    native_window: NativeWindow,
    focused: AtomicBool,
    running: AtomicBool,
}

#[cfg(target_os = "android")]
impl AndroidWindow {
    /// Create a new Android window from a native window
    pub fn new(native_window: NativeWindow) -> Self {
        Self {
            native_window,
            focused: AtomicBool::new(true),
            running: AtomicBool::new(true),
        }
    }

    /// Get the underlying native window
    pub fn native_window(&self) -> &NativeWindow {
        &self.native_window
    }

    /// Set the focused state
    pub(crate) fn set_focused(&self, focused: bool) {
        self.focused.store(focused, Ordering::Relaxed);
    }

    /// Set the running state
    pub(crate) fn set_running(&self, running: bool) {
        self.running.store(running, Ordering::Relaxed);
    }

    /// Check if the window is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

#[cfg(target_os = "android")]
impl Window for AndroidWindow {
    fn size(&self) -> (u32, u32) {
        (
            self.native_window.width() as u32,
            self.native_window.height() as u32,
        )
    }

    fn logical_size(&self) -> (f32, f32) {
        // Android handles DPI internally through the view system
        // For now, return physical size as logical size
        let (w, h) = self.size();
        (w as f32, h as f32)
    }

    fn scale_factor(&self) -> f64 {
        // TODO: Get actual density from DisplayMetrics
        // For now, return 1.0 as Android handles scaling internally
        1.0
    }

    fn set_title(&self, _title: &str) {
        // Android apps don't have window titles in the traditional sense
        // Title is set via AndroidManifest.xml or Activity APIs
    }

    fn set_cursor(&self, _cursor: Cursor) {
        // Mobile platforms don't typically use cursor icons
        // Touch input doesn't need a visible cursor
    }

    fn request_redraw(&self) {
        // Android renders continuously when focused
        // Redraw requests are handled by the event loop
    }

    fn is_focused(&self) -> bool {
        self.focused.load(Ordering::Relaxed)
    }

    fn is_visible(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

#[cfg(target_os = "android")]
unsafe impl Send for AndroidWindow {}
#[cfg(target_os = "android")]
unsafe impl Sync for AndroidWindow {}

/// Placeholder for non-Android builds
#[cfg(not(target_os = "android"))]
pub struct AndroidWindow {
    _private: (),
}

#[cfg(not(target_os = "android"))]
impl AndroidWindow {
    /// Create a placeholder window (panics on non-Android)
    pub fn new() -> Self {
        Self { _private: () }
    }
}

#[cfg(not(target_os = "android"))]
impl Window for AndroidWindow {
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
