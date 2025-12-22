//! Desktop window implementation using winit

use blinc_platform::{Cursor, Window, WindowConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window as WinitWindow, WindowAttributes};

/// Desktop window wrapping a winit window
pub struct DesktopWindow {
    window: Arc<WinitWindow>,
    focused: AtomicBool,
}

impl DesktopWindow {
    /// Create a new desktop window
    pub fn new(
        event_loop: &ActiveEventLoop,
        config: &WindowConfig,
    ) -> Result<Self, winit::error::OsError> {
        let mut attrs = WindowAttributes::default()
            .with_title(&config.title)
            .with_inner_size(LogicalSize::new(config.width, config.height))
            .with_resizable(config.resizable)
            .with_decorations(config.decorations)
            .with_transparent(config.transparent);

        if config.fullscreen {
            attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }

        let window = event_loop.create_window(attrs)?;

        Ok(Self {
            window: Arc::new(window),
            focused: AtomicBool::new(true),
        })
    }

    /// Get the underlying winit window
    pub fn winit_window(&self) -> &WinitWindow {
        &self.window
    }

    /// Get an Arc to the winit window
    pub fn winit_window_arc(&self) -> Arc<WinitWindow> {
        Arc::clone(&self.window)
    }

    /// Set focus state (called by event loop)
    pub(crate) fn set_focused(&self, focused: bool) {
        self.focused.store(focused, Ordering::Relaxed);
    }
}

impl Window for DesktopWindow {
    fn size(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width, size.height)
    }

    fn logical_size(&self) -> (f32, f32) {
        let size = self.window.inner_size();
        let scale = self.window.scale_factor();
        (
            (size.width as f64 / scale) as f32,
            (size.height as f64 / scale) as f32,
        )
    }

    fn scale_factor(&self) -> f64 {
        self.window.scale_factor()
    }

    fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }

    fn set_cursor(&self, cursor: Cursor) {
        use winit::window::CursorIcon;
        let icon = match cursor {
            Cursor::Default => CursorIcon::Default,
            Cursor::Pointer => CursorIcon::Pointer,
            Cursor::Text => CursorIcon::Text,
            Cursor::Crosshair => CursorIcon::Crosshair,
            Cursor::Move => CursorIcon::Move,
            Cursor::NotAllowed => CursorIcon::NotAllowed,
            Cursor::ResizeNS => CursorIcon::NsResize,
            Cursor::ResizeEW => CursorIcon::EwResize,
            Cursor::ResizeNESW => CursorIcon::NeswResize,
            Cursor::ResizeNWSE => CursorIcon::NwseResize,
            Cursor::Grab => CursorIcon::Grab,
            Cursor::Grabbing => CursorIcon::Grabbing,
            Cursor::Wait => CursorIcon::Wait,
            Cursor::Progress => CursorIcon::Progress,
            Cursor::None => {
                self.window.set_cursor_visible(false);
                return;
            }
        };
        self.window.set_cursor_visible(true);
        self.window.set_cursor(icon);
    }

    fn request_redraw(&self) {
        self.window.request_redraw();
    }

    fn is_focused(&self) -> bool {
        self.focused.load(Ordering::Relaxed)
    }

    fn is_visible(&self) -> bool {
        self.window.is_visible().unwrap_or(true)
    }
}

// Safety: Window operations are thread-safe via winit's internal synchronization
unsafe impl Send for DesktopWindow {}
unsafe impl Sync for DesktopWindow {}
