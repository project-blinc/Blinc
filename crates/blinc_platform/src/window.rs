//! Window abstraction and configuration

/// Window configuration
#[derive(Clone, Debug)]
pub struct WindowConfig {
    /// Window title
    pub title: String,
    /// Initial width in logical pixels
    pub width: u32,
    /// Initial height in logical pixels
    pub height: u32,
    /// Whether the window can be resized
    pub resizable: bool,
    /// Whether to show window decorations (title bar, borders)
    pub decorations: bool,
    /// Whether the window should be transparent
    pub transparent: bool,
    /// Whether the window should always be on top
    pub always_on_top: bool,
    /// Whether to start in fullscreen mode
    pub fullscreen: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Blinc App".to_string(),
            width: 800,
            height: 600,
            resizable: true,
            decorations: true,
            transparent: false,
            always_on_top: false,
            fullscreen: false,
        }
    }
}

impl WindowConfig {
    /// Create a new window configuration with a title
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    /// Set the window title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set the window size
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Set whether the window is resizable
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Set whether to show window decorations
    pub fn decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }

    /// Set whether the window is transparent
    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    /// Set whether the window is always on top
    pub fn always_on_top(mut self, always_on_top: bool) -> Self {
        self.always_on_top = always_on_top;
        self
    }

    /// Set whether to start in fullscreen
    pub fn fullscreen(mut self, fullscreen: bool) -> Self {
        self.fullscreen = fullscreen;
        self
    }
}

/// Window abstraction trait
///
/// Implemented by platform-specific window types.
pub trait Window: Send {
    /// Get window size in physical pixels
    fn size(&self) -> (u32, u32);

    /// Get window size in logical pixels
    fn logical_size(&self) -> (f32, f32);

    /// Get the display scale factor (DPI scaling)
    fn scale_factor(&self) -> f64;

    /// Set the window title
    fn set_title(&self, title: &str);

    /// Set the cursor icon
    fn set_cursor(&self, cursor: Cursor);

    /// Request a redraw
    fn request_redraw(&self);

    /// Check if the window is focused
    fn is_focused(&self) -> bool;

    /// Check if the window is visible
    fn is_visible(&self) -> bool;
}

/// Cursor icons
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cursor {
    /// Default arrow cursor
    #[default]
    Default,
    /// Pointer/hand cursor (for clickable elements)
    Pointer,
    /// Text/I-beam cursor (for text input)
    Text,
    /// Crosshair cursor
    Crosshair,
    /// Move cursor (for dragging)
    Move,
    /// Not allowed cursor
    NotAllowed,
    /// North-South resize cursor
    ResizeNS,
    /// East-West resize cursor
    ResizeEW,
    /// Northeast-Southwest resize cursor
    ResizeNESW,
    /// Northwest-Southeast resize cursor
    ResizeNWSE,
    /// Grab cursor (open hand)
    Grab,
    /// Grabbing cursor (closed hand)
    Grabbing,
    /// Wait/loading cursor
    Wait,
    /// Progress cursor (arrow with spinner)
    Progress,
    /// Hidden cursor
    None,
}
