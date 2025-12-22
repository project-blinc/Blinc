//! Desktop event loop implementation using winit

use crate::input;
use crate::window::DesktopWindow;
use blinc_platform::{
    ControlFlow, Event, EventLoop, LifecycleEvent, PlatformError, Window, WindowConfig, WindowEvent,
};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent as WinitWindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop as WinitEventLoop};
use winit::keyboard::ModifiersState;
use winit::window::WindowId;

/// Desktop event loop wrapping winit's event loop
pub struct DesktopEventLoop {
    event_loop: WinitEventLoop<()>,
    window_config: WindowConfig,
}

impl DesktopEventLoop {
    /// Create a new desktop event loop
    pub fn new(config: WindowConfig) -> Result<Self, PlatformError> {
        let event_loop = WinitEventLoop::new()
            .map_err(|e| PlatformError::EventLoop(e.to_string()))?;

        Ok(Self {
            event_loop,
            window_config: config,
        })
    }
}

impl EventLoop for DesktopEventLoop {
    type Window = DesktopWindow;

    fn run<F>(self, handler: F) -> Result<(), PlatformError>
    where
        F: FnMut(Event, &Self::Window) -> ControlFlow + 'static,
    {
        let mut app = DesktopApp::new(self.window_config, handler);
        self.event_loop
            .run_app(&mut app)
            .map_err(|e| PlatformError::EventLoop(e.to_string()))
    }
}

/// Internal winit application handler
struct DesktopApp<F>
where
    F: FnMut(Event, &DesktopWindow) -> ControlFlow,
{
    window_config: WindowConfig,
    window: Option<DesktopWindow>,
    handler: F,
    modifiers: ModifiersState,
    mouse_position: (f32, f32),
    should_exit: bool,
}

impl<F> DesktopApp<F>
where
    F: FnMut(Event, &DesktopWindow) -> ControlFlow,
{
    fn new(window_config: WindowConfig, handler: F) -> Self {
        Self {
            window_config,
            window: None,
            handler,
            modifiers: ModifiersState::empty(),
            mouse_position: (0.0, 0.0),
            should_exit: false,
        }
    }

    fn handle_event(&mut self, event: Event) {
        if let Some(ref window) = self.window {
            let flow = (self.handler)(event, window);
            if flow == ControlFlow::Exit {
                self.should_exit = true;
            }
        }
    }
}

impl<F> ApplicationHandler for DesktopApp<F>
where
    F: FnMut(Event, &DesktopWindow) -> ControlFlow,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window if we don't have one
        if self.window.is_none() {
            match DesktopWindow::new(event_loop, &self.window_config) {
                Ok(window) => {
                    self.window = Some(window);
                    self.handle_event(Event::Lifecycle(LifecycleEvent::Resumed));
                }
                Err(e) => {
                    tracing::error!("Failed to create window: {}", e);
                    event_loop.exit();
                }
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.handle_event(Event::Lifecycle(LifecycleEvent::Suspended));
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        // Request redraw on wait timeout (frame tick)
        if matches!(cause, StartCause::WaitCancelled { .. } | StartCause::Poll) {
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WinitWindowEvent,
    ) {
        match event {
            WinitWindowEvent::CloseRequested => {
                self.handle_event(Event::Window(WindowEvent::CloseRequested));
                if self.should_exit {
                    event_loop.exit();
                }
            }

            WinitWindowEvent::Resized(size) => {
                self.handle_event(Event::Window(WindowEvent::Resized {
                    width: size.width,
                    height: size.height,
                }));
            }

            WinitWindowEvent::Moved(pos) => {
                self.handle_event(Event::Window(WindowEvent::Moved {
                    x: pos.x,
                    y: pos.y,
                }));
            }

            WinitWindowEvent::Focused(focused) => {
                if let Some(ref window) = self.window {
                    window.set_focused(focused);
                }
                self.handle_event(Event::Window(WindowEvent::Focused(focused)));
            }

            WinitWindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.handle_event(Event::Window(WindowEvent::ScaleFactorChanged {
                    scale_factor,
                }));
            }

            WinitWindowEvent::RedrawRequested => {
                self.handle_event(Event::Frame);
                if self.should_exit {
                    event_loop.exit();
                }
            }

            WinitWindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }

            WinitWindowEvent::KeyboardInput { event, .. } => {
                let input_event =
                    input::convert_keyboard_event(&event.logical_key, event.state, self.modifiers);
                self.handle_event(Event::Input(input_event));
            }

            WinitWindowEvent::CursorMoved { position, .. } => {
                self.mouse_position = (position.x as f32, position.y as f32);
                let input_event = input::mouse_moved(self.mouse_position.0, self.mouse_position.1);
                self.handle_event(Event::Input(input_event));
            }

            WinitWindowEvent::MouseInput { state, button, .. } => {
                let (x, y) = self.mouse_position;
                let input_event = match state {
                    winit::event::ElementState::Pressed => input::mouse_pressed(button, x, y),
                    winit::event::ElementState::Released => input::mouse_released(button, x, y),
                };
                self.handle_event(Event::Input(input_event));
            }

            WinitWindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x, y),
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        (pos.x as f32 / 10.0, pos.y as f32 / 10.0)
                    }
                };
                let input_event = input::scroll_event(dx, dy);
                self.handle_event(Event::Input(input_event));
            }

            WinitWindowEvent::Touch(touch) => {
                let input_event = input::convert_touch_event(&touch);
                self.handle_event(Event::Input(input_event));
            }

            WinitWindowEvent::CursorEntered { .. } => {
                self.handle_event(Event::Input(blinc_platform::InputEvent::Mouse(
                    blinc_platform::MouseEvent::Entered,
                )));
            }

            WinitWindowEvent::CursorLeft { .. } => {
                self.handle_event(Event::Input(blinc_platform::InputEvent::Mouse(
                    blinc_platform::MouseEvent::Left,
                )));
            }

            _ => {}
        }

        // Check for exit
        if self.should_exit {
            event_loop.exit();
        }
    }

    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        self.handle_event(Event::Lifecycle(LifecycleEvent::LowMemory));
    }
}
