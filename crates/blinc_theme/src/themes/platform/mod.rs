//! Platform-native themes
//!
//! Each platform has its own native look and feel:
//! - macOS: Aqua/Big Sur design language
//! - Windows: Fluent Design System
//! - Linux: Adwaita (GNOME)
//! - iOS and Android: Blinc's own Universal HID Expressive
//!   ([`crate::themes::ExpressiveTheme`]); there is no iOS or Android theme
//! - Web: Universal HID Hybrid ([`crate::themes::HybridTheme`])
//!
//! When no platform matches (custom targets, headless contexts,
//! fallback paths), [`platform_theme_bundle`] returns the Universal
//! HID Hybrid bundle — a considered Apple HIG / Material 3 synthesis
//! that reads native on every platform. [`crate::themes::BlincTheme`]
//! is now a type alias for `HybridTheme`; the previous Catppuccin
//! palette has been retired.

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_arch = "wasm32")]
pub mod web;

use crate::theme::ThemeBundle;

/// Get the appropriate theme bundle for the current platform
pub fn platform_theme_bundle() -> ThemeBundle {
    #[cfg(target_os = "macos")]
    {
        return macos::MacOSTheme::bundle();
    }

    #[cfg(target_os = "windows")]
    {
        return windows::WindowsTheme::bundle();
    }

    #[cfg(target_os = "linux")]
    {
        return linux::LinuxTheme::bundle();
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        return crate::themes::ExpressiveTheme::bundle();
    }

    #[cfg(target_arch = "wasm32")]
    {
        return web::WebTheme::bundle();
    }

    // Fallback for any context where no platform module compiled
    // in (custom targets, headless tooling). Returns the Universal
    // HID Hybrid bundle — designed as the cross-platform default.
    #[allow(unreachable_code)]
    {
        crate::themes::HybridTheme::bundle()
    }
}
