//! Android Activity integration
//!
//! Provides the main entry point for Android applications and handles
//! the Android activity lifecycle events.

#[cfg(target_os = "android")]
use android_activity::{AndroidApp, MainEvent, PollEvent};

#[cfg(target_os = "android")]
use ndk::native_window::NativeWindow;

#[cfg(target_os = "android")]
use tracing::{debug, info, warn};

/// Android application state
#[cfg(target_os = "android")]
pub struct BlincAndroidApp {
    window: Option<NativeWindow>,
    running: bool,
    focused: bool,
}

#[cfg(target_os = "android")]
impl BlincAndroidApp {
    /// Create a new Blinc Android application
    pub fn new() -> Self {
        Self {
            window: None,
            running: true,
            focused: false,
        }
    }

    /// Handle Android events
    pub fn handle_event(&mut self, app: &AndroidApp, event: PollEvent) {
        match event {
            PollEvent::Main(main_event) => {
                self.handle_main_event(app, main_event);
            }
            _ => {}
        }
    }

    /// Handle main lifecycle events
    fn handle_main_event(&mut self, app: &AndroidApp, event: MainEvent) {
        match event {
            MainEvent::InitWindow { .. } => {
                info!("Native window initialized");
                // Get the native window
                if let Some(window) = app.native_window() {
                    let width = window.width();
                    let height = window.height();
                    info!("Window size: {}x{}", width, height);
                    self.window = Some(window);
                    self.init_graphics();
                }
            }

            MainEvent::TerminateWindow { .. } => {
                info!("Native window terminated");
                self.window = None;
            }

            MainEvent::WindowResized { .. } => {
                if let Some(ref window) = self.window {
                    let width = window.width();
                    let height = window.height();
                    info!("Window resized: {}x{}", width, height);
                    // TODO: Handle resize in GPU renderer
                }
            }

            MainEvent::GainedFocus => {
                info!("App gained focus");
                self.focused = true;
            }

            MainEvent::LostFocus => {
                info!("App lost focus");
                self.focused = false;
            }

            MainEvent::Pause => {
                info!("App paused");
                self.focused = false;
            }

            MainEvent::Resume { .. } => {
                info!("App resumed");
                self.focused = true;
            }

            MainEvent::Start => {
                info!("App started");
            }

            MainEvent::Stop => {
                info!("App stopped");
            }

            MainEvent::Destroy => {
                info!("App destroyed");
                self.running = false;
            }

            MainEvent::SaveState { .. } => {
                debug!("Saving app state");
                // TODO: Save reactive state
            }

            MainEvent::ConfigChanged { .. } => {
                debug!("Configuration changed");
            }

            MainEvent::LowMemory => {
                warn!("Low memory warning");
                // TODO: Release caches
            }

            MainEvent::ContentRectChanged { .. } => {
                debug!("Content rect changed");
            }

            _ => {}
        }
    }

    /// Initialize graphics (GPU renderer)
    fn init_graphics(&mut self) {
        if let Some(ref _window) = self.window {
            // TODO: Initialize wgpu with the native window
            // This will use blinc_gpu with Vulkan backend
            info!("Graphics initialization placeholder");
        }
    }

    /// Render a frame
    pub fn render_frame(&mut self) {
        // TODO: Render using blinc_gpu
        // 1. Process reactive updates
        // 2. Update animations
        // 3. Layout widgets
        // 4. Paint to GPU
    }

    /// Check if app is running
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Check if we should render
    pub fn should_render(&self) -> bool {
        self.window.is_some() && self.focused
    }
}

#[cfg(target_os = "android")]
impl Default for BlincAndroidApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Android main entry point
///
/// This is called by the android-activity crate when the app starts.
/// This is only enabled when the "default-activity" feature is enabled.
/// Applications should typically provide their own android_main and use
/// blinc_app::AndroidApp::run() instead.
#[cfg(all(target_os = "android", feature = "default-activity"))]
#[no_mangle]
pub fn android_main(app: AndroidApp) {
    // Initialize Android logging
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("Blinc"),
    );

    info!("android_main called");

    let mut blinc_app = BlincAndroidApp::new();

    while blinc_app.is_running() {
        // Poll for events with 16ms timeout (roughly 60fps)
        app.poll_events(Some(std::time::Duration::from_millis(16)), |event| {
            blinc_app.handle_event(&app, event);
        });

        // Render if we have a window and are focused
        if blinc_app.should_render() {
            blinc_app.render_frame();
        }
    }

    info!("Blinc Android app shutting down");
}

/// Placeholder for non-Android builds (allows cross-compilation checks)
#[cfg(not(target_os = "android"))]
pub fn android_main() {
    // This is never called - just allows the code to compile on non-Android
}

#[cfg(test)]
mod tests {
    // Tests run on host, not on Android
    #[test]
    fn test_placeholder() {
        // Android-specific code can't be tested on host
        assert!(true);
    }
}
