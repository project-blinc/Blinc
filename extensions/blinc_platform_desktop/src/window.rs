//! Desktop window implementation using winit

use blinc_platform::{Cursor, Window, WindowConfig, WindowLevel};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window as WinitWindow, WindowAttributes, WindowLevel as WinitWindowLevel};

/// Desktop window wrapping a winit window
pub struct DesktopWindow {
    accessibility: crate::accessibility::DesktopAccessibility,
    window: Arc<WinitWindow>,
    focused: AtomicBool,
    occluded: AtomicBool,
    /// Whether this window was configured as transparent. Cached at
    /// creation time because winit doesn't expose a way to query it
    /// back, and the GPU runner needs it to pick the wgpu
    /// `CompositeAlphaMode` at surface-config time.
    transparent: bool,
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
            .with_transparent(config.transparent)
            .with_maximized(config.maximized)
            .with_visible(false)
            .with_active(config.active)
            .with_window_level(map_window_level(config.window_level));

        if config.fullscreen {
            attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
        if let Some((min_w, min_h)) = config.min_size {
            attrs = attrs.with_min_inner_size(LogicalSize::new(min_w, min_h));
        }
        if let Some((max_w, max_h)) = config.max_size {
            attrs = attrs.with_max_inner_size(LogicalSize::new(max_w, max_h));
        }
        if let Some((x, y)) = config.position {
            attrs = attrs.with_position(winit::dpi::LogicalPosition::new(x, y));
        }

        attrs = apply_platform_attrs(attrs, config);

        let window = Arc::new(event_loop.create_window(attrs)?);
        let accessibility = crate::accessibility::DesktopAccessibility::new(event_loop, &window);
        apply_window_level(&window, config.window_level);

        // Center window on screen if requested
        if config.center {
            if let Some(monitor) = window.current_monitor() {
                let screen = monitor.size();
                let win_size = window.outer_size();
                let x = (screen.width.saturating_sub(win_size.width)) / 2;
                let y = (screen.height.saturating_sub(win_size.height)) / 2;
                window.set_outer_position(winit::dpi::PhysicalPosition::new(x as i32, y as i32));
            }
        }

        // IME — winit's `with_ime_allowed` exists only on the
        // attributes for some platforms; the portable path is to set
        // it post-create.
        window.set_ime_allowed(config.ime_allowed);
        if !config.cursor_visible {
            window.set_cursor_visible(false);
        }

        window.set_visible(config.visible);
        Ok(Self {
            accessibility,
            window,
            focused: AtomicBool::new(config.active),
            occluded: AtomicBool::new(false),
            transparent: config.transparent,
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

    /// Whether this window was configured with a transparent surface.
    pub fn is_transparent(&self) -> bool {
        self.transparent
    }

    pub(crate) fn process_accessibility_event(&self, event: &winit::event::WindowEvent) {
        self.accessibility.process_event(&self.window, event);
    }

    pub fn accessibility_active(&self) -> bool {
        self.accessibility.is_active()
    }
    pub fn accessibility_pending(&self) -> bool {
        self.accessibility.is_pending()
    }
    pub fn accessibility_generation(&self) -> u64 {
        self.accessibility.generation()
    }
    pub fn take_accessibility_request(&self) -> Option<crate::AccessibilityRequest> {
        self.accessibility.take()
    }
    pub fn update_accessibility(&self, update: impl FnOnce() -> accesskit::TreeUpdate) {
        self.accessibility.update(update);
    }
    pub fn is_occluded(&self) -> bool {
        self.occluded.load(Ordering::Acquire)
    }
    pub(crate) fn set_occluded(&self, occluded: bool) {
        self.occluded.store(occluded, Ordering::Release);
    }

    /// Set focus state (called by event loop)
    pub(crate) fn set_focused(&self, focused: bool) {
        self.focused.store(focused, Ordering::Relaxed);
    }
}

impl Window for DesktopWindow {
    fn id(&self) -> blinc_platform::WindowId {
        // Use winit's WindowId hash as our platform-agnostic ID
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.window.id().hash(&mut hasher);
        blinc_platform::WindowId(hasher.finish())
    }

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

    fn pre_present_notify(&self) {
        // Forwards to winit; on Wayland this registers the
        // `wl_surface::frame()` callback so the next `RedrawRequested`
        // waits for the compositor's frame-ready signal instead of
        // racing it (which blocks `surface.get_current_texture()` for
        // ~1 s per acquire — the Linux "frozen UI" symptom).
        self.window.pre_present_notify();
    }

    fn is_focused(&self) -> bool {
        self.focused.load(Ordering::Relaxed)
    }

    fn is_visible(&self) -> bool {
        self.window.is_visible().unwrap_or(true)
    }

    fn set_position(&self, x: i32, y: i32) {
        self.window
            .set_outer_position(winit::dpi::LogicalPosition::new(x, y));
    }

    fn center_on_screen(&self) {
        if let Some(monitor) = self.window.current_monitor() {
            let screen = monitor.size();
            let win_size = self.window.outer_size();
            let x = (screen.width.saturating_sub(win_size.width)) / 2;
            let y = (screen.height.saturating_sub(win_size.height)) / 2;
            self.window
                .set_outer_position(winit::dpi::PhysicalPosition::new(x as i32, y as i32));
        }
    }

    fn set_size(&self, width: u32, height: u32) {
        let _ = self
            .window
            .request_inner_size(LogicalSize::new(width, height));
    }

    fn drag_window(&self) {
        let _ = self.window.drag_window();
    }

    fn minimize(&self) {
        self.window.set_minimized(true);
    }

    fn maximize(&self) {
        self.window.set_maximized(!self.window.is_maximized());
    }

    fn close(&self) {
        // winit doesn't have a close method — we request the event loop to close
        // by not requesting redraws. The actual close is handled by the event loop
        // when it receives CloseRequested.
        self.window.set_visible(false);
    }

    fn set_window_level(&self, level: WindowLevel) {
        apply_window_level(&self.window, level);
    }

    fn is_transparent(&self) -> bool {
        self.transparent
    }
}

// Safety: Window operations are thread-safe via winit's internal synchronization
unsafe impl Send for DesktopWindow {}
unsafe impl Sync for DesktopWindow {}

fn map_window_level(level: WindowLevel) -> WinitWindowLevel {
    match level {
        WindowLevel::AlwaysOnBottom => WinitWindowLevel::AlwaysOnBottom,
        WindowLevel::Normal => WinitWindowLevel::Normal,
        WindowLevel::AlwaysOnTop => WinitWindowLevel::AlwaysOnTop,
    }
}

fn apply_window_level(window: &WinitWindow, level: WindowLevel) {
    let level = map_window_level(level);
    // Creation attributes seed winit's cached state on some backends;
    // toggling through Normal forces a real platform update.
    if level != WinitWindowLevel::Normal {
        window.set_window_level(WinitWindowLevel::Normal);
    }
    window.set_window_level(level);
}

/// Apply platform-specific extension attributes (macOS title-bar
/// styling, Windows taskbar / shadow / class, X11 WM_CLASS). The
/// `cfg(target_os)` gates keep cross-platform builds from pulling in
/// extension traits they don't need.
#[allow(unused_mut, unused_variables, clippy::let_and_return)]
fn apply_platform_attrs(mut attrs: WindowAttributes, config: &WindowConfig) -> WindowAttributes {
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowAttributesExtMacOS;
        if config.macos_titlebar_transparent {
            attrs = attrs.with_titlebar_transparent(true);
        }
        if config.macos_titlebar_hidden {
            attrs = attrs.with_titlebar_hidden(true);
        }
        if config.macos_titlebar_buttons_hidden {
            attrs = attrs.with_titlebar_buttons_hidden(true);
        }
        if config.macos_fullsize_content_view {
            attrs = attrs.with_fullsize_content_view(true);
        }
        if config.macos_movable_by_window_background {
            attrs = attrs.with_movable_by_window_background(true);
        }
    }
    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::WindowAttributesExtWindows;
        if config.windows_skip_taskbar {
            attrs = attrs.with_skip_taskbar(true);
        }
        if config.windows_undecorated_shadow {
            attrs = attrs.with_undecorated_shadow(true);
        }
        // Windows drag_and_drop is enabled by default in winit;
        // forward the config flag only when it'd actually change
        // the default to avoid surprising users on older drivers.
        if !config.windows_drag_and_drop {
            attrs = attrs.with_drag_and_drop(false);
        }
        if let Some(class) = &config.windows_class_name {
            attrs = attrs.with_class_name(class.clone());
        }
    }
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        use winit::platform::x11::WindowAttributesExtX11;
        if let Some((instance, class)) = &config.x11_class {
            attrs = attrs.with_name(instance.clone(), class.clone());
        }
    }
    attrs
}
