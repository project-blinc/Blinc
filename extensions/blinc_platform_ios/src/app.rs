//! iOS application integration
//!
//! Provides the iOS platform implementation and entry point.

use crate::event_loop::IOSEventLoop;
use crate::window::IOSWindow;
use blinc_platform::{Platform, PlatformError};

#[cfg(target_os = "ios")]
use objc2_ui_kit::UIScreen;

#[cfg(target_os = "ios")]
use tracing::info;

/// iOS platform implementation
pub struct IOSPlatform {
    /// Display scale factor
    #[cfg(target_os = "ios")]
    scale_factor: f64,
    #[cfg(not(target_os = "ios"))]
    _private: (),
}

#[cfg(target_os = "ios")]
impl IOSPlatform {
    /// Get the main screen's scale factor
    fn get_screen_scale() -> f64 {
        unsafe {
            let screen = UIScreen::mainScreen();
            screen.scale() as f64
        }
    }
}

impl Platform for IOSPlatform {
    type Window = IOSWindow;
    type EventLoop = IOSEventLoop;

    #[cfg(target_os = "ios")]
    fn new() -> Result<Self, PlatformError> {
        let scale_factor = Self::get_screen_scale();
        info!(
            "iOS platform initialized with scale factor: {}",
            scale_factor
        );
        Ok(Self { scale_factor })
    }

    #[cfg(not(target_os = "ios"))]
    fn new() -> Result<Self, PlatformError> {
        Err(PlatformError::Unsupported(
            "iOS platform only available on iOS".to_string(),
        ))
    }

    fn create_event_loop(&self) -> Result<Self::EventLoop, PlatformError> {
        Ok(IOSEventLoop::new())
    }

    fn name(&self) -> &'static str {
        "ios"
    }

    #[cfg(target_os = "ios")]
    fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    #[cfg(not(target_os = "ios"))]
    fn scale_factor(&self) -> f64 {
        1.0
    }
}

/// iOS main entry point
///
/// This function is called by the iOS app delegate when the application launches.
/// It initializes the Blinc runtime and sets up the Metal rendering context.
///
/// Note: On iOS, the actual event loop is managed by UIKit's RunLoop.
/// This function sets up the necessary infrastructure for Blinc to integrate
/// with the iOS lifecycle.
#[cfg(target_os = "ios")]
pub fn ios_main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("iOS main entry point called");

    // Note: The actual application lifecycle is managed by UIApplicationDelegate
    // and UIKit. This function serves as the Rust-side initialization point.
    //
    // A typical iOS app using Blinc would:
    // 1. Create a UIWindow with a UIViewController
    // 2. Set up a CAMetalLayer on the view
    // 3. Create a CADisplayLink for frame callbacks
    // 4. Route touch events to Blinc's input system
    //
    // See IOSApp::run() in blinc_app for the full integration.
}

/// Placeholder for non-iOS builds
#[cfg(not(target_os = "ios"))]
pub fn ios_main() {
    // Placeholder - iOS main is only called on iOS
}

/// Get the display scale factor for the main screen
#[cfg(target_os = "ios")]
pub fn get_display_scale() -> f64 {
    IOSPlatform::get_screen_scale()
}

/// Placeholder for non-iOS builds
#[cfg(not(target_os = "ios"))]
pub fn get_display_scale() -> f64 {
    1.0
}

/// Check if the system is in dark mode
#[cfg(target_os = "ios")]
pub fn is_dark_mode() -> bool {
    unsafe {
        let screen = UIScreen::mainScreen();
        // Check the traitCollection for dark mode
        // UIUserInterfaceStyle: 0 = unspecified, 1 = light, 2 = dark
        let trait_collection = screen.traitCollection();
        trait_collection.userInterfaceStyle().0 == 2 // UIUserInterfaceStyleDark
    }
}

/// Placeholder for non-iOS builds
#[cfg(not(target_os = "ios"))]
pub fn is_dark_mode() -> bool {
    false
}

/// Get the safe area insets for the main screen
///
/// Returns (top, left, bottom, right) insets in logical points.
#[cfg(target_os = "ios")]
pub fn get_safe_area_insets() -> (f32, f32, f32, f32) {
    // Get key window's safe area insets
    // This accounts for notch, home indicator, etc.
    // Note: In a real implementation, you'd get this from the UIWindow
    (0.0, 0.0, 0.0, 0.0)
}

/// Placeholder for non-iOS builds
#[cfg(not(target_os = "ios"))]
pub fn get_safe_area_insets() -> (f32, f32, f32, f32) {
    (0.0, 0.0, 0.0, 0.0)
}

/// iOS system font paths
pub fn system_font_paths() -> &'static [&'static str] {
    &[
        "/System/Library/Fonts/SFNSText.ttf",
        "/System/Library/Fonts/SFNSDisplay.ttf",
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/Core/AppleSystemUIFont.ttf",
        "/System/Library/Fonts/Core/Helvetica.ttc",
    ]
}
