//! Desktop event loop implementation using winit
//!
//! Supports multiple windows via `AppCommand::CreateWindow`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::input;
use crate::window::DesktopWindow;
use blinc_platform::{
    ControlFlow, Event, EventLoop, LifecycleEvent, PlatformError, Window, WindowConfig,
    WindowEvent, WindowId,
};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent as WinitWindowEvent};
use winit::event_loop::{
    ActiveEventLoop, ControlFlow as WinitControlFlow, EventLoop as WinitEventLoop, EventLoopProxy,
};
use winit::keyboard::ModifiersState;
use winit::window::WindowId as WinitWindowId;

/// Commands sent to the event loop via `EventLoopProxy`.
///
/// Used for cross-thread requests that require access to `ActiveEventLoop`
/// (e.g., creating new windows, which can only happen inside the event handler).
#[derive(Debug)]
pub enum AppCommand {
    /// Wake the event loop (request redraw on all windows)
    Wake,
    /// Create a new window with the given configuration
    CreateWindow(WindowConfig),
    /// Close a specific window
    CloseWindow(WindowId),
}

/// Proxy for waking up the event loop from another thread
///
/// Use this to request a redraw from a background animation thread,
/// or to send commands for window creation/destruction.
#[derive(Clone)]
pub struct WakeProxy {
    proxy: EventLoopProxy<AppCommand>,
    /// Shared deadline + lazy timer thread for `wake_at`. Lazily
    /// initialised on first deadline-based wake to avoid spawning a
    /// timer thread for apps that never use the animation FPS cap.
    timer: Arc<TimerState>,
}

/// State for the lazy timer thread that backs `WakeProxy::wake_at`.
struct TimerState {
    /// `(deadline, condvar)`. Setting the deadline + notifying the
    /// condvar wakes the timer thread which then sleeps via
    /// `wait_timeout` until the deadline expires.
    deadline: std::sync::Mutex<Option<std::time::Instant>>,
    cv: std::sync::Condvar,
    started: AtomicBool,
}

impl WakeProxy {
    /// Wake up the event loop, causing it to process events and potentially redraw
    pub fn wake(&self) {
        let _ = self.proxy.send_event(AppCommand::Wake);
    }

    /// Schedule a `wake()` to fire after `delay`. If a wake is already
    /// pending and would fire sooner, this call is a no-op (we never
    /// extend an earlier deadline).
    ///
    /// Backs the windowed app's `animation_fps_cap` — the redraw
    /// chain uses this instead of `request_redraw()` when the only
    /// reason to schedule a frame is animation progress and the app
    /// has asked for a sub-vsync animation rate. A single dedicated
    /// timer thread is started lazily on first call; subsequent
    /// calls just update the deadline.
    pub fn wake_at(&self, delay: std::time::Duration) {
        let target = std::time::Instant::now() + delay;
        let mut guard = self.timer.deadline.lock().unwrap();
        match *guard {
            Some(existing) if existing <= target => {
                // An earlier wake is already pending — keep it.
                return;
            }
            _ => *guard = Some(target),
        }
        self.timer.cv.notify_one();
        drop(guard);

        if !self.timer.started.swap(true, Ordering::AcqRel) {
            let timer = Arc::clone(&self.timer);
            let proxy = self.proxy.clone();
            std::thread::Builder::new()
                .name("blinc-wake-timer".to_string())
                .spawn(move || {
                    let mut guard = timer.deadline.lock().unwrap();
                    loop {
                        match *guard {
                            None => {
                                // Park until a deadline is set.
                                guard = timer.cv.wait(guard).unwrap();
                            }
                            Some(d) => {
                                let now = std::time::Instant::now();
                                if d <= now {
                                    *guard = None;
                                    drop(guard);
                                    let _ = proxy.send_event(AppCommand::Wake);
                                    guard = timer.deadline.lock().unwrap();
                                } else {
                                    let timeout = d - now;
                                    let (g, _) = timer.cv.wait_timeout(guard, timeout).unwrap();
                                    guard = g;
                                }
                            }
                        }
                    }
                })
                .expect("spawn blinc-wake-timer");
        }
    }

    /// Request creation of a new window on the next event loop tick
    pub fn create_window(&self, config: WindowConfig) {
        let _ = self.proxy.send_event(AppCommand::CreateWindow(config));
    }

    /// Request closing a specific window
    pub fn close_window(&self, id: WindowId) {
        let _ = self.proxy.send_event(AppCommand::CloseWindow(id));
    }
}

/// Desktop event loop wrapping winit's event loop
pub struct DesktopEventLoop {
    event_loop: WinitEventLoop<AppCommand>,
    window_config: WindowConfig,
    wake_proxy: WakeProxy,
}

impl DesktopEventLoop {
    /// Create a new desktop event loop
    pub fn new(config: WindowConfig) -> Result<Self, PlatformError> {
        let event_loop = WinitEventLoop::with_user_event()
            .build()
            .map_err(|e| PlatformError::EventLoop(e.to_string()))?;

        let wake_proxy = WakeProxy {
            proxy: event_loop.create_proxy(),
            timer: Arc::new(TimerState {
                deadline: std::sync::Mutex::new(None),
                cv: std::sync::Condvar::new(),
                started: AtomicBool::new(false),
            }),
        };

        Ok(Self {
            event_loop,
            window_config: config,
            wake_proxy,
        })
    }

    /// Get a wake proxy that can be used to wake up the event loop from another thread
    ///
    /// This is useful for animation threads that need to request redraws,
    /// or for creating new windows from background tasks.
    pub fn wake_proxy(&self) -> WakeProxy {
        self.wake_proxy.clone()
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

/// Convert a winit WindowId to our platform-agnostic WindowId
fn to_window_id(winit_id: WinitWindowId) -> WindowId {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    winit_id.hash(&mut hasher);
    WindowId(hasher.finish())
}

/// Internal winit application handler supporting multiple windows
struct DesktopApp<F>
where
    F: FnMut(Event, &DesktopWindow) -> ControlFlow,
{
    /// Config for the primary (initial) window
    window_config: WindowConfig,
    /// All open windows keyed by winit's WindowId
    windows: HashMap<WinitWindowId, DesktopWindow>,
    /// The primary window's winit ID (first window created)
    primary_winit_id: Option<WinitWindowId>,
    /// Event handler
    handler: F,
    /// Current keyboard modifiers
    modifiers: ModifiersState,
    /// Current mouse position (per-window tracking could be added later)
    mouse_position: (f32, f32),
    /// Whether the app should exit
    should_exit: bool,
    /// Currently active modal window (blocks input to other windows)
    modal_window: Option<WinitWindowId>,
}

impl<F> DesktopApp<F>
where
    F: FnMut(Event, &DesktopWindow) -> ControlFlow,
{
    fn new(window_config: WindowConfig, handler: F) -> Self {
        Self {
            window_config,
            windows: HashMap::new(),
            primary_winit_id: None,
            handler,
            modifiers: ModifiersState::empty(),
            mouse_position: (0.0, 0.0),
            should_exit: false,
            modal_window: None,
        }
    }

    /// Dispatch an event using the window identified by winit_id
    fn handle_event_for(&mut self, winit_id: WinitWindowId, event: Event) {
        if let Some(window) = self.windows.get(&winit_id) {
            let flow = (self.handler)(event, window);
            if flow == ControlFlow::Exit {
                self.should_exit = true;
            }
        }
    }

    /// Dispatch an event using the primary window (for global events)
    fn handle_event(&mut self, event: Event) {
        if let Some(primary_id) = self.primary_winit_id {
            self.handle_event_for(primary_id, event);
        }
    }

    /// Create a new window and register it
    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        config: &WindowConfig,
    ) -> Option<WinitWindowId> {
        match DesktopWindow::new(event_loop, config) {
            Ok(window) => {
                let winit_id = window.winit_window().id();
                self.windows.insert(winit_id, window);
                Some(winit_id)
            }
            Err(e) => {
                tracing::error!("Failed to create window: {}", e);
                None
            }
        }
    }
}

impl<F> ApplicationHandler<AppCommand> for DesktopApp<F>
where
    F: FnMut(Event, &DesktopWindow) -> ControlFlow,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Wait until something actually happens. winit 0.30 defaults to
        // `ControlFlow::Poll`, which on X11 / Wayland makes the event
        // loop spin as fast as the kernel will schedule it (issue #28
        // — 25 % idle CPU on Ubuntu 25.10 / Intel HD 520). macOS hides
        // this because NSApp.run is event-driven even under Poll, so
        // the bug never showed up there. Under `Wait` the loop blocks
        // until input arrives, the scheduler's `wake_proxy.wake()`
        // fires (delivered as `StartCause::WaitCancelled`), or a
        // window asks for a redraw — none of which happen on a static
        // focused UI, so the loop sleeps and the process drops to
        // ~0 % CPU.
        event_loop.set_control_flow(WinitControlFlow::Wait);

        // Create the primary window if we don't have one
        if self.primary_winit_id.is_none() {
            let config = self.window_config.clone();
            if let Some(winit_id) = self.create_window(event_loop, &config) {
                self.primary_winit_id = Some(winit_id);
                self.handle_event_for(winit_id, Event::Lifecycle(LifecycleEvent::Resumed));
            } else {
                event_loop.exit();
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.handle_event(Event::Lifecycle(LifecycleEvent::Suspended));
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        // Wake-cause telemetry. Silent in normal builds (the format
        // args aren't even evaluated when the trace target is
        // disabled). For idle-CPU diagnosis on Linux, run with
        // `RUST_LOG=blinc_platform_desktop::wakes=trace` and the
        // output shows one line per event-loop wake — useful for
        // counting wakeups/sec on a static UI.
        tracing::trace!(
            target: "blinc_platform_desktop::wakes",
            ?cause,
            "event loop woke"
        );

        // Intentionally no `request_redraw` here.
        //
        // Earlier this handler called `request_redraw()` on every
        // window for every `WaitCancelled` cause. That blanket wake
        // showed up as residual idle CPU on Linux (issue #28): Wayland
        // / X11 compositors deliver more spurious wakes than macOS
        // — focus subscriptions, configure events, raw input shifts
        // — and each wake fired a redraw the windowed app's frame
        // gate then immediately threw away because `frame_dirty` was
        // false. The wakeup-and-skip cycle is cheap individually but
        // the OS overhead added up to a few percent of a CPU even on
        // a static, out-of-focus hello-world.
        //
        // Two paths actually need the redraw, and both arrive at it
        // without our help here:
        //
        // - `wake_proxy.wake()` (animation scheduler bg thread, FPS
        //   cap timer, external wake calls) sends
        //   `AppCommand::Wake`. winit delivers that as `user_event`
        //   below, which already calls `request_redraw` on every
        //   window. The bg-thread side also flips `frame_dirty` to
        //   `true` before sending, so the resulting `Event::Frame`
        //   actually paints.
        //
        // - Real `WindowEvent` / `DeviceEvent` wakes flow through
        //   `window_event` (and the windowed-app handler's prelude),
        //   which decides per-event whether to flip `frame_dirty`
        //   and call `request_redraw`. Bare mouse-moves are
        //   intentionally skipped; everything else paints exactly
        //   once.
        //
        // Anything reaching `WaitCancelled` that *isn't* one of the
        // two above is by definition a wake we don't need to act
        // on. Letting it return through `about_to_wait` and back to
        // `ControlFlow::Wait` keeps the process at ~0% CPU.
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        winit_id: WinitWindowId,
        event: WinitWindowEvent,
    ) {
        // Block input to non-modal windows when a modal is active
        if let Some(modal_id) = self.modal_window {
            if winit_id != modal_id {
                // Allow close/resize/redraw but block input events
                match &event {
                    WinitWindowEvent::KeyboardInput { .. }
                    | WinitWindowEvent::MouseInput { .. }
                    | WinitWindowEvent::CursorMoved { .. }
                    | WinitWindowEvent::MouseWheel { .. }
                    | WinitWindowEvent::Touch(_) => return,
                    _ => {}
                }
            }
        }

        let wid = to_window_id(winit_id);

        match event {
            WinitWindowEvent::CloseRequested => {
                self.handle_event_for(winit_id, Event::Window(wid, WindowEvent::CloseRequested));

                // Remove the window
                self.windows.remove(&winit_id);

                // Clear modal if the closed window was the modal
                if self.modal_window == Some(winit_id) {
                    self.modal_window = None;
                }

                // If no windows remain, exit
                if self.windows.is_empty() {
                    self.should_exit = true;
                    event_loop.exit();
                }
            }

            WinitWindowEvent::Resized(size) => {
                self.handle_event_for(
                    winit_id,
                    Event::Window(
                        wid,
                        WindowEvent::Resized {
                            width: size.width,
                            height: size.height,
                        },
                    ),
                );
            }

            WinitWindowEvent::Moved(pos) => {
                self.handle_event_for(
                    winit_id,
                    Event::Window(wid, WindowEvent::Moved { x: pos.x, y: pos.y }),
                );
            }

            WinitWindowEvent::Focused(focused) => {
                if let Some(window) = self.windows.get(&winit_id) {
                    window.set_focused(focused);
                }
                self.handle_event_for(winit_id, Event::Window(wid, WindowEvent::Focused(focused)));
            }

            WinitWindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.handle_event_for(
                    winit_id,
                    Event::Window(wid, WindowEvent::ScaleFactorChanged { scale_factor }),
                );
            }

            WinitWindowEvent::RedrawRequested => {
                self.handle_event_for(winit_id, Event::Frame(wid));
                if self.should_exit {
                    event_loop.exit();
                }
            }

            WinitWindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
                // Forward the change as an InputEvent so app-level
                // consumers can keep their own modifier cache in
                // sync — winit doesn't always fire KeyboardInput
                // for the modifier key transitions themselves
                // (focus-dependent on some backends), so without
                // this dedicated forward a Shift release that
                // happens while a non-canvas element holds focus
                // leaves the app's cache latched at `shift: true`.
                let mods_event = blinc_platform::InputEvent::ModifiersChanged(
                    input::convert_modifiers(self.modifiers),
                );
                self.handle_event_for(winit_id, Event::Input(wid, mods_event));
            }

            WinitWindowEvent::KeyboardInput { event, .. } => {
                let input_event = input::convert_keyboard_event(
                    &event.logical_key,
                    event.state,
                    self.modifiers,
                    event.text.as_deref(),
                );
                self.handle_event_for(winit_id, Event::Input(wid, input_event));
                if let Some(window) = self.windows.get(&winit_id) {
                    window.request_redraw();
                }
            }

            WinitWindowEvent::CursorMoved { position, .. } => {
                self.mouse_position = (position.x as f32, position.y as f32);
                let input_event = input::mouse_moved(self.mouse_position.0, self.mouse_position.1);
                self.handle_event_for(winit_id, Event::Input(wid, input_event));
            }

            WinitWindowEvent::MouseInput { state, button, .. } => {
                let (x, y) = self.mouse_position;
                let input_event = match state {
                    winit::event::ElementState::Pressed => input::mouse_pressed(button, x, y),
                    winit::event::ElementState::Released => input::mouse_released(button, x, y),
                };
                self.handle_event_for(winit_id, Event::Input(wid, input_event));
            }

            WinitWindowEvent::MouseWheel { delta, phase, .. } => {
                // `LineDelta` is emitted by classic step-wheel mice; each tick
                // is ±1 line. `PixelDelta` is emitted by trackpads / Magic Mouse
                // and already gives pixel-accurate scroll distance per event
                // (NSScrollView semantics on macOS, and equivalent on other
                // platforms). Line-based events need to be converted to pixels
                // using a per-line height — we use 40px to match what most
                // native apps feel like. Pixel deltas must be passed through
                // unchanged; a previous divide-by-10 here caused a perceptible
                // drag on trackpads (only 10 % of the intended scroll applied).
                const LINE_HEIGHT_PX: f32 = 40.0;
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => {
                        (x * LINE_HEIGHT_PX, y * LINE_HEIGHT_PX)
                    }
                    winit::event::MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                };
                let input_event = input::scroll_event(dx, dy, phase);
                self.handle_event_for(winit_id, Event::Input(wid, input_event));

                if matches!(
                    phase,
                    winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled
                ) {
                    self.handle_event_for(winit_id, Event::Input(wid, input::scroll_end_event()));
                }
            }

            WinitWindowEvent::Touch(touch) => {
                let input_event = input::convert_touch_event(&touch);
                self.handle_event_for(winit_id, Event::Input(wid, input_event));
            }

            WinitWindowEvent::CursorEntered { .. } => {
                self.handle_event_for(
                    winit_id,
                    Event::Input(
                        wid,
                        blinc_platform::InputEvent::Mouse(blinc_platform::MouseEvent::Entered),
                    ),
                );
            }

            WinitWindowEvent::CursorLeft { .. } => {
                self.handle_event_for(
                    winit_id,
                    Event::Input(
                        wid,
                        blinc_platform::InputEvent::Mouse(blinc_platform::MouseEvent::Left),
                    ),
                );
            }

            WinitWindowEvent::DroppedFile(path) => {
                self.handle_event_for(
                    winit_id,
                    Event::Window(wid, WindowEvent::DroppedFile { paths: vec![path] }),
                );
            }

            WinitWindowEvent::HoveredFile(path) => {
                self.handle_event_for(
                    winit_id,
                    Event::Window(wid, WindowEvent::DroppedFileHovered { paths: vec![path] }),
                );
            }

            WinitWindowEvent::HoveredFileCancelled => {
                self.handle_event_for(
                    winit_id,
                    Event::Window(wid, WindowEvent::DroppedFileCancelled),
                );
            }

            WinitWindowEvent::Ime(ime_event) => {
                match ime_event {
                    winit::event::Ime::Commit(text) => {
                        // IME committed text — deliver each character as a Char key event
                        for c in text.chars() {
                            let input_event = blinc_platform::InputEvent::Keyboard(
                                blinc_platform::KeyboardEvent {
                                    key: blinc_platform::Key::Char(c),
                                    state: blinc_platform::KeyState::Pressed,
                                    modifiers: blinc_platform::Modifiers::default(),
                                    text: None,
                                },
                            );
                            self.handle_event_for(winit_id, Event::Input(wid, input_event));
                        }
                        if let Some(window) = self.windows.get(&winit_id) {
                            window.request_redraw();
                        }
                    }
                    winit::event::Ime::Preedit(text, cursor) => {
                        // IME pre-edit (composition in progress)
                        // TODO: render pre-edit text with underline at cursor position
                        let _ = (text, cursor);
                    }
                    winit::event::Ime::Enabled => {
                        tracing::debug!("IME enabled for window {:?}", winit_id);
                    }
                    winit::event::Ime::Disabled => {
                        tracing::debug!("IME disabled for window {:?}", winit_id);
                    }
                }
            }

            WinitWindowEvent::PinchGesture { delta, phase, .. } => {
                let scroll_phase = match phase {
                    winit::event::TouchPhase::Started => blinc_platform::ScrollPhase::Started,
                    winit::event::TouchPhase::Moved => blinc_platform::ScrollPhase::Moved,
                    winit::event::TouchPhase::Ended => blinc_platform::ScrollPhase::Ended,
                    winit::event::TouchPhase::Cancelled => blinc_platform::ScrollPhase::Ended,
                };
                self.handle_event_for(
                    winit_id,
                    Event::Input(
                        wid,
                        blinc_platform::InputEvent::Pinch {
                            scale: 1.0 + delta as f32,
                            phase: scroll_phase,
                        },
                    ),
                );
            }

            WinitWindowEvent::RotationGesture { delta, phase, .. } => {
                let scroll_phase = match phase {
                    winit::event::TouchPhase::Started => blinc_platform::ScrollPhase::Started,
                    winit::event::TouchPhase::Moved => blinc_platform::ScrollPhase::Moved,
                    winit::event::TouchPhase::Ended => blinc_platform::ScrollPhase::Ended,
                    winit::event::TouchPhase::Cancelled => blinc_platform::ScrollPhase::Ended,
                };
                self.handle_event_for(
                    winit_id,
                    Event::Input(
                        wid,
                        blinc_platform::InputEvent::Rotation {
                            angle: delta.to_radians(),
                            phase: scroll_phase,
                        },
                    ),
                );
            }

            _ => {}
        }

        if self.should_exit {
            event_loop.exit();
        }
    }

    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        self.handle_event(Event::Lifecycle(LifecycleEvent::LowMemory));
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, command: AppCommand) {
        match command {
            AppCommand::Wake => {
                // Wake from animation thread — request redraw on all windows
                for window in self.windows.values() {
                    window.request_redraw();
                }
            }
            AppCommand::CreateWindow(config) => {
                let is_modal = config.modal;
                if let Some(winit_id) = self.create_window(event_loop, &config) {
                    if is_modal {
                        self.modal_window = Some(winit_id);
                    }
                    let wid = to_window_id(winit_id);
                    self.handle_event_for(winit_id, Event::Lifecycle(LifecycleEvent::Resumed));
                    tracing::info!(
                        "Created new window {:?} (wid={:?}, modal={})",
                        winit_id,
                        wid,
                        is_modal
                    );
                }
            }
            AppCommand::CloseWindow(wid) => {
                // Find the winit ID for this WindowId
                let winit_id = self
                    .windows
                    .iter()
                    .find(|(_, w)| w.id() == wid)
                    .map(|(id, _)| *id);
                if let Some(winit_id) = winit_id {
                    self.handle_event_for(
                        winit_id,
                        Event::Window(wid, WindowEvent::CloseRequested),
                    );
                    self.windows.remove(&winit_id);

                    // Clear modal if the closed window was the modal
                    if self.modal_window == Some(winit_id) {
                        self.modal_window = None;
                    }

                    if self.windows.is_empty() {
                        self.should_exit = true;
                        event_loop.exit();
                    }
                }
            }
        }
    }
}
