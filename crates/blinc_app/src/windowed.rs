//! Windowed application runner
//!
//! Provides a unified API for running windowed Blinc applications across
//! desktop and Android platforms.
//!
//! # Example
//!
//! ```ignore
//! use blinc_app::prelude::*;
//! use blinc_app::windowed::WindowedApp;
//!
//! fn main() -> Result<()> {
//!     WindowedApp::run(WindowConfig::default(), |ctx| {
//!         // Build your UI
//!         div().w_full().h_full()
//!             .flex_center()
//!             .child(text("Hello Blinc!").size(48.0))
//!     })
//! }
//! ```

use blinc_layout::prelude::*;
use blinc_platform::{
    ControlFlow, Event, EventLoop, InputEvent, KeyState, LifecycleEvent, MouseEvent, Platform,
    TouchEvent, Window, WindowConfig, WindowEvent,
};

use crate::app::BlincApp;
use crate::error::{BlincError, Result};

#[cfg(all(feature = "windowed", not(target_os = "android")))]
use blinc_platform_desktop::DesktopPlatform;

/// Context passed to the UI builder function
pub struct WindowedContext {
    /// Current window width in physical pixels (matches surface size)
    pub width: f32,
    /// Current window height in physical pixels (matches surface size)
    pub height: f32,
    /// Current scale factor (physical / logical)
    pub scale_factor: f64,
    /// Whether the window is focused
    pub focused: bool,
    /// Event router for input event handling
    pub event_router: EventRouter,
}

impl WindowedContext {
    fn from_window<W: Window>(window: &W, event_router: EventRouter) -> Self {
        // Use physical size for rendering - the surface is in physical pixels
        // UI layout and rendering must use physical dimensions to match the surface
        let (width, height) = window.size();
        Self {
            width: width as f32,
            height: height as f32,
            scale_factor: window.scale_factor(),
            focused: window.is_focused(),
            event_router,
        }
    }

    /// Update context from window (preserving event router)
    fn update_from_window<W: Window>(&mut self, window: &W) {
        let (width, height) = window.size();
        self.width = width as f32;
        self.height = height as f32;
        self.scale_factor = window.scale_factor();
        self.focused = window.is_focused();
    }
}

/// Windowed application runner
///
/// Provides a simple way to run a Blinc application in a window
/// with automatic event handling and rendering.
pub struct WindowedApp;

impl WindowedApp {
    /// Initialize the platform asset loader
    ///
    /// On desktop, this sets up a filesystem-based loader.
    /// On Android, this would use the NDK AssetManager.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    fn init_asset_loader() {
        use blinc_platform::assets::{set_global_asset_loader, FilesystemAssetLoader};

        // Create a filesystem loader (uses current directory as base)
        let loader = FilesystemAssetLoader::new();

        // Try to set the global loader (ignore error if already set)
        let _ = set_global_asset_loader(Box::new(loader));
    }

    /// Run a windowed Blinc application on desktop platforms
    ///
    /// This is the main entry point for desktop applications. It creates
    /// a window, sets up GPU rendering, and runs the event loop.
    ///
    /// # Arguments
    ///
    /// * `config` - Window configuration (title, size, etc.)
    /// * `ui_builder` - Function that builds the UI tree given the window context
    ///
    /// # Example
    ///
    /// ```ignore
    /// WindowedApp::run(WindowConfig::default(), |ctx| {
    ///     div()
    ///         .w(ctx.width).h(ctx.height)
    ///         .bg([0.1, 0.1, 0.15, 1.0])
    ///         .flex_center()
    ///         .child(
    ///             div().glass().rounded(16.0).p(24.0)
    ///                 .child(text("Hello Blinc!").size(32.0))
    ///         )
    /// })
    /// ```
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    pub fn run<F, E>(config: WindowConfig, ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder,
    {
        Self::run_desktop(config, ui_builder)
    }

    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    fn run_desktop<F, E>(config: WindowConfig, mut ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder,
    {
        // Initialize the platform asset loader for cross-platform asset loading
        Self::init_asset_loader();

        let platform = DesktopPlatform::new().map_err(|e| BlincError::Platform(e.to_string()))?;
        let event_loop = platform
            .create_event_loop_with_config(config)
            .map_err(|e| BlincError::Platform(e.to_string()))?;

        // We need to defer BlincApp creation until we have a window
        let mut app: Option<BlincApp> = None;
        let mut surface: Option<wgpu::Surface<'static>> = None;
        let mut surface_config: Option<wgpu::SurfaceConfiguration> = None;

        // Persistent context with event router
        let mut ctx: Option<WindowedContext> = None;
        // Persistent render tree for hit testing
        let mut render_tree: Option<RenderTree> = None;
        // Track if we need to rebuild UI (e.g., after state change)
        let mut needs_rebuild = true;

        event_loop
            .run(move |event, window| {
                match event {
                    Event::Lifecycle(LifecycleEvent::Resumed) => {
                        // Initialize GPU if not already done
                        if app.is_none() {
                            let winit_window = window.winit_window_arc();

                            match BlincApp::with_window(winit_window, None) {
                                Ok((blinc_app, surf)) => {
                                    let (width, height) = window.size();
                                    // Use the same texture format that the renderer's pipelines use
                                    let format = blinc_app.texture_format();
                                    let config = wgpu::SurfaceConfiguration {
                                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                                        format,
                                        width,
                                        height,
                                        present_mode: wgpu::PresentMode::AutoVsync,
                                        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                                        view_formats: vec![],
                                        desired_maximum_frame_latency: 2,
                                    };
                                    surf.configure(&blinc_app.device(), &config);

                                    surface = Some(surf);
                                    surface_config = Some(config);
                                    app = Some(blinc_app);

                                    // Initialize context with event router
                                    ctx = Some(WindowedContext::from_window(
                                        window,
                                        EventRouter::new(),
                                    ));

                                    tracing::info!("Blinc windowed app initialized");
                                }
                                Err(e) => {
                                    tracing::error!("Failed to initialize Blinc: {}", e);
                                    return ControlFlow::Exit;
                                }
                            }
                        }
                    }

                    Event::Window(WindowEvent::Resized { width, height }) => {
                        if let (Some(ref blinc_app), Some(ref surf), Some(ref mut config)) =
                            (&app, &surface, &mut surface_config)
                        {
                            if width > 0 && height > 0 {
                                config.width = width;
                                config.height = height;
                                surf.configure(&blinc_app.device(), config);
                                needs_rebuild = true;
                            }
                        }
                    }

                    Event::Window(WindowEvent::CloseRequested) => {
                        return ControlFlow::Exit;
                    }

                    // Handle input events
                    Event::Input(input_event) => {
                        if let (Some(ref mut windowed_ctx), Some(ref tree)) =
                            (&mut ctx, &render_tree)
                        {
                            let router = &mut windowed_ctx.event_router;

                            match input_event {
                                InputEvent::Mouse(mouse_event) => match mouse_event {
                                    MouseEvent::Moved { x, y } => {
                                        router.on_mouse_move(tree, x, y);
                                    }
                                    MouseEvent::ButtonPressed { button, x, y } => {
                                        let btn = convert_mouse_button(button);
                                        router.on_mouse_down(tree, x, y, btn);
                                    }
                                    MouseEvent::ButtonReleased { button, x, y } => {
                                        let btn = convert_mouse_button(button);
                                        router.on_mouse_up(tree, x, y, btn);
                                    }
                                    MouseEvent::Left => {
                                        router.on_mouse_leave();
                                    }
                                    MouseEvent::Entered => {
                                        // Re-trigger hover on enter
                                        let (mx, my) = router.mouse_position();
                                        router.on_mouse_move(tree, mx, my);
                                    }
                                },
                                InputEvent::Keyboard(kb_event) => match kb_event.state {
                                    KeyState::Pressed => {
                                        router.on_key_down(0); // TODO: proper key code
                                    }
                                    KeyState::Released => {
                                        router.on_key_up(0);
                                    }
                                },
                                InputEvent::Touch(touch_event) => {
                                    // Map touch to mouse events for simplicity
                                    match touch_event {
                                        TouchEvent::Started { x, y, .. } => {
                                            router.on_mouse_down(tree, x, y, MouseButton::Left);
                                        }
                                        TouchEvent::Moved { x, y, .. } => {
                                            router.on_mouse_move(tree, x, y);
                                        }
                                        TouchEvent::Ended { x, y, .. } => {
                                            router.on_mouse_up(tree, x, y, MouseButton::Left);
                                        }
                                        TouchEvent::Cancelled { .. } => {
                                            router.on_mouse_leave();
                                        }
                                    }
                                }
                                InputEvent::Scroll { delta_x, delta_y } => {
                                    router.on_scroll(tree, delta_x, delta_y);
                                }
                            }
                        }
                    }

                    Event::Frame => {
                        if let (
                            Some(ref mut blinc_app),
                            Some(ref surf),
                            Some(ref config),
                            Some(ref mut windowed_ctx),
                        ) = (&mut app, &surface, &surface_config, &mut ctx)
                        {
                            // Get current frame
                            let frame = match surf.get_current_texture() {
                                Ok(f) => f,
                                Err(wgpu::SurfaceError::Lost) => {
                                    surf.configure(&blinc_app.device(), config);
                                    return ControlFlow::Continue;
                                }
                                Err(wgpu::SurfaceError::OutOfMemory) => {
                                    tracing::error!("Out of GPU memory");
                                    return ControlFlow::Exit;
                                }
                                Err(e) => {
                                    tracing::warn!("Surface error: {:?}", e);
                                    return ControlFlow::Continue;
                                }
                            };

                            let view = frame
                                .texture
                                .create_view(&wgpu::TextureViewDescriptor::default());

                            // Update context from window
                            windowed_ctx.update_from_window(window);

                            // Build UI - context provides physical size and event router
                            let ui = ui_builder(windowed_ctx);

                            // Build render tree for hit testing (needed for event routing)
                            if needs_rebuild || render_tree.is_none() {
                                let mut tree = RenderTree::from_element(&ui);
                                tree.compute_layout(windowed_ctx.width, windowed_ctx.height);
                                render_tree = Some(tree);
                                needs_rebuild = false;
                            }

                            // Render at physical size (matches surface dimensions)
                            if let Err(e) =
                                blinc_app.render(&ui, &view, windowed_ctx.width, windowed_ctx.height)
                            {
                                tracing::error!("Render error: {}", e);
                            }

                            frame.present();
                        }
                    }

                    _ => {}
                }

                ControlFlow::Continue
            })
            .map_err(|e| BlincError::Platform(e.to_string()))?;

        Ok(())
    }

    /// Placeholder for non-windowed builds
    #[cfg(not(feature = "windowed"))]
    pub fn run<F, E>(_config: WindowConfig, _ui_builder: F) -> Result<()>
    where
        F: FnMut(&mut WindowedContext) -> E + 'static,
        E: ElementBuilder,
    {
        Err(BlincError::Platform(
            "Windowed feature not enabled. Add 'windowed' feature to blinc_app".to_string(),
        ))
    }
}

/// Convert platform mouse button to layout mouse button
#[cfg(all(feature = "windowed", not(target_os = "android")))]
fn convert_mouse_button(button: blinc_platform::MouseButton) -> MouseButton {
    match button {
        blinc_platform::MouseButton::Left => MouseButton::Left,
        blinc_platform::MouseButton::Right => MouseButton::Right,
        blinc_platform::MouseButton::Middle => MouseButton::Middle,
        blinc_platform::MouseButton::Back => MouseButton::Back,
        blinc_platform::MouseButton::Forward => MouseButton::Forward,
        blinc_platform::MouseButton::Other(n) => MouseButton::Other(n),
    }
}

/// Convenience function to run a windowed app with default configuration
#[cfg(feature = "windowed")]
pub fn run_windowed<F, E>(ui_builder: F) -> Result<()>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder,
{
    WindowedApp::run(WindowConfig::default(), ui_builder)
}

/// Convenience function to run a windowed app with a title
#[cfg(feature = "windowed")]
pub fn run_windowed_with_title<F, E>(title: &str, ui_builder: F) -> Result<()>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder,
{
    let config = WindowConfig {
        title: title.to_string(),
        ..Default::default()
    };
    WindowedApp::run(config, ui_builder)
}
