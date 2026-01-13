//! Android event loop implementation
//!
//! Wraps the android-activity event polling to implement the blinc_platform EventLoop trait.

use crate::window::AndroidWindow;
use blinc_platform::{
    ControlFlow, Event, EventLoop, LifecycleEvent, PlatformError, Window, WindowEvent,
};

#[cfg(target_os = "android")]
use android_activity::{AndroidApp, MainEvent, PollEvent};

#[cfg(target_os = "android")]
use ndk::looper::ForeignLooper;

#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "android")]
use std::sync::Arc;

#[cfg(target_os = "android")]
use std::time::Duration;

#[cfg(target_os = "android")]
use tracing::{debug, info, warn};

/// Proxy for waking up the event loop from another thread
///
/// Use this to request a redraw from a background animation thread.
/// Call `wake()` to send a wake-up signal to the event loop.
#[cfg(target_os = "android")]
#[derive(Clone)]
pub struct AndroidWakeProxy {
    /// The looper to wake
    looper: ForeignLooper,
    /// Flag indicating a wake was requested
    wake_requested: Arc<AtomicBool>,
}

#[cfg(target_os = "android")]
impl AndroidWakeProxy {
    /// Create a new wake proxy for the current thread's looper
    pub fn new() -> Option<Self> {
        ForeignLooper::for_thread().map(|looper| Self {
            looper,
            wake_requested: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Wake up the event loop, causing it to process events and potentially redraw
    pub fn wake(&self) {
        self.wake_requested.store(true, Ordering::SeqCst);
        self.looper.wake();
    }

    /// Check if a wake was requested and clear the flag
    pub fn take_wake_request(&self) -> bool {
        self.wake_requested.swap(false, Ordering::SeqCst)
    }
}

/// Placeholder for non-Android builds
#[cfg(not(target_os = "android"))]
#[derive(Clone)]
pub struct AndroidWakeProxy;

#[cfg(not(target_os = "android"))]
impl AndroidWakeProxy {
    /// Create a placeholder wake proxy
    pub fn new() -> Option<Self> {
        None
    }

    /// No-op wake for non-Android
    pub fn wake(&self) {}

    /// Always returns false on non-Android
    pub fn take_wake_request(&self) -> bool {
        false
    }
}

/// Android event loop wrapping android-activity's polling
#[cfg(target_os = "android")]
pub struct AndroidEventLoop {
    app: AndroidApp,
    wake_proxy: Option<AndroidWakeProxy>,
}

#[cfg(target_os = "android")]
impl AndroidEventLoop {
    /// Create a new Android event loop
    pub fn new(app: AndroidApp) -> Self {
        // Create wake proxy - this captures the current thread's looper
        let wake_proxy = AndroidWakeProxy::new();
        if wake_proxy.is_none() {
            warn!("Failed to create AndroidWakeProxy - animations may not wake event loop");
        }
        Self { app, wake_proxy }
    }

    /// Get a wake proxy that can be used to wake up the event loop from another thread
    ///
    /// This is useful for animation threads that need to request redraws.
    /// Returns None if the looper couldn't be obtained (shouldn't happen in normal operation).
    pub fn wake_proxy(&self) -> Option<AndroidWakeProxy> {
        self.wake_proxy.clone()
    }
}

#[cfg(target_os = "android")]
impl EventLoop for AndroidEventLoop {
    type Window = AndroidWindow;

    fn run<F>(self, mut handler: F) -> Result<(), PlatformError>
    where
        F: FnMut(Event, &Self::Window) -> ControlFlow + 'static,
    {
        let mut window: Option<AndroidWindow> = None;
        let mut should_exit = false;

        while !should_exit {
            // Poll for events with 16ms timeout (~60fps)
            self.app
                .poll_events(Some(Duration::from_millis(16)), |event| match event {
                    PollEvent::Main(main_event) => match main_event {
                        MainEvent::InitWindow { .. } => {
                            info!("Android: Native window initialized");
                            if let Some(native) = self.app.native_window() {
                                let w = native.width();
                                let h = native.height();
                                info!("Android: Window size {}x{}", w, h);
                                window = Some(AndroidWindow::new(native));
                            }
                        }

                        MainEvent::TerminateWindow { .. } => {
                            info!("Android: Native window terminated");
                            window = None;
                        }

                        MainEvent::WindowResized { .. } => {
                            if let Some(ref win) = window {
                                let (width, height) = win.size();
                                info!("Android: Window resized to {}x{}", width, height);
                                let flow = handler(
                                    Event::Window(WindowEvent::Resized { width, height }),
                                    win,
                                );
                                if flow == ControlFlow::Exit {
                                    should_exit = true;
                                }
                            }
                        }

                        MainEvent::GainedFocus => {
                            debug!("Android: Gained focus");
                            if let Some(ref win) = window {
                                win.set_focused(true);
                                let flow = handler(Event::Window(WindowEvent::Focused(true)), win);
                                if flow == ControlFlow::Exit {
                                    should_exit = true;
                                }
                            }
                        }

                        MainEvent::LostFocus => {
                            debug!("Android: Lost focus");
                            if let Some(ref win) = window {
                                win.set_focused(false);
                                let flow = handler(Event::Window(WindowEvent::Focused(false)), win);
                                if flow == ControlFlow::Exit {
                                    should_exit = true;
                                }
                            }
                        }

                        MainEvent::Resume { .. } => {
                            info!("Android: Resumed");
                            if let Some(ref win) = window {
                                let flow = handler(Event::Lifecycle(LifecycleEvent::Resumed), win);
                                if flow == ControlFlow::Exit {
                                    should_exit = true;
                                }
                            }
                        }

                        MainEvent::Pause => {
                            info!("Android: Paused");
                            if let Some(ref win) = window {
                                let flow =
                                    handler(Event::Lifecycle(LifecycleEvent::Suspended), win);
                                if flow == ControlFlow::Exit {
                                    should_exit = true;
                                }
                            }
                        }

                        MainEvent::Destroy => {
                            info!("Android: Destroyed");
                            if let Some(ref win) = window {
                                win.set_running(false);
                                let flow = handler(Event::Window(WindowEvent::CloseRequested), win);
                                if flow == ControlFlow::Exit {
                                    should_exit = true;
                                }
                            }
                            should_exit = true;
                        }

                        MainEvent::LowMemory => {
                            warn!("Android: Low memory");
                            if let Some(ref win) = window {
                                let flow =
                                    handler(Event::Lifecycle(LifecycleEvent::LowMemory), win);
                                if flow == ControlFlow::Exit {
                                    should_exit = true;
                                }
                            }
                        }

                        _ => {}
                    },
                    _ => {}
                });

            // Check if animation thread requested a wake
            let wake_requested = self
                .wake_proxy
                .as_ref()
                .map(|p| p.take_wake_request())
                .unwrap_or(false);

            // Frame tick when we have a focused window or a wake was requested
            if let Some(ref win) = window {
                if win.is_focused() || wake_requested {
                    let flow = handler(Event::Frame, win);
                    if flow == ControlFlow::Exit {
                        should_exit = true;
                    }
                }
            }
        }

        info!("Android: Event loop exiting");
        Ok(())
    }
}

/// Placeholder for non-Android builds
#[cfg(not(target_os = "android"))]
pub struct AndroidEventLoop {
    _private: (),
}

#[cfg(not(target_os = "android"))]
impl AndroidEventLoop {
    /// Create a placeholder event loop (for cross-compilation checks)
    pub fn new() -> Self {
        Self { _private: () }
    }
}

#[cfg(not(target_os = "android"))]
impl EventLoop for AndroidEventLoop {
    type Window = AndroidWindow;

    fn run<F>(self, _handler: F) -> Result<(), PlatformError>
    where
        F: FnMut(Event, &Self::Window) -> ControlFlow + 'static,
    {
        Err(PlatformError::Unsupported(
            "Android platform only available on Android".to_string(),
        ))
    }
}
