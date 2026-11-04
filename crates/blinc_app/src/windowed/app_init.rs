//! Setup helpers for the desktop runner: assets, theme, window actions, alpha mode.

use crate::windowed::WindowedApp;
use std::hash::Hash;
use std::sync::Arc;

impl WindowedApp {
    /// Initialize the platform asset loader
    ///
    /// On desktop, this sets up a filesystem-based loader.
    /// On Android, this would use the NDK AssetManager.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    pub(super) fn init_asset_loader() {
        use blinc_platform::assets::{FilesystemAssetLoader, set_global_asset_loader};

        // Create a filesystem loader (uses current directory as base)
        let loader = FilesystemAssetLoader::new();

        // Try to set the global loader (ignore error if already set)
        let _ = set_global_asset_loader(Box::new(loader));
    }

    /// Initialize the theme system with platform detection
    ///
    /// This sets up the global ThemeState with:
    /// - Platform-appropriate theme bundle (macOS, Windows, Linux, etc.)
    /// - System color scheme detection (light/dark mode)
    /// - Redraw callback to trigger UI updates on theme changes
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    pub(super) fn init_theme() {
        use blinc_theme::{
            ThemeState, detect_system_color_scheme, platform_theme_bundle, set_redraw_callback,
        };

        // Only seed the platform default if no theme has been
        // installed yet — users who call `ThemeState::init` before
        // `WindowedApp::run` keep their bundle. Users who call it
        // *after* (e.g. from inside the UI builder) are handled by
        // `ThemeState::init`'s replace-when-already-set path.
        if ThemeState::try_get().is_none() {
            let bundle = platform_theme_bundle();
            let scheme = detect_system_color_scheme();
            ThemeState::init(bundle, scheme);
        }

        // Register the redraw callback unconditionally — needed for
        // every theme swap (color scheme toggle, runtime bundle
        // replace) regardless of who installed the initial theme.
        // Previously this lived inside the `is_none()` branch, so a
        // pre-init user theme silently disabled CSS reparse + tree
        // rebuild on subsequent theme changes.
        set_redraw_callback(|| {
            tracing::debug!("Theme changed - requesting full rebuild + CSS reparse");
            blinc_layout::widgets::request_css_reparse();
            blinc_layout::widgets::request_full_rebuild();
        });
    }

    /// Create per-window action closures (close, drag, minimize, maximize).
    /// Returns (close, drag, minimize, maximize) Arcs.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    #[allow(clippy::type_complexity)]
    pub(super) fn make_window_actions(
        win: std::sync::Arc<winit::window::Window>,
        wake: blinc_platform_desktop::WakeProxy,
    ) -> (
        Arc<dyn Fn() + Send + Sync>,
        Arc<dyn Fn() + Send + Sync>,
        Arc<dyn Fn() + Send + Sync>,
        Arc<dyn Fn() + Send + Sync>,
    ) {
        let d = Arc::downgrade(&win);
        let mi = Arc::downgrade(&win);
        let ma = Arc::downgrade(&win);
        let cl = Arc::downgrade(&win);
        let wake_for_close = wake;
        (
            Arc::new(move || {
                if let Some(w) = cl.upgrade() {
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    w.id().hash(&mut hasher);
                    wake_for_close.close_window(blinc_platform::WindowId(hasher.finish()));
                }
            }),
            Arc::new(move || {
                if let Some(w) = d.upgrade() {
                    let _ = w.drag_window();
                }
            }),
            Arc::new(move || {
                if let Some(w) = mi.upgrade() {
                    w.set_minimized(true);
                }
            }),
            Arc::new(move || {
                if let Some(w) = ma.upgrade() {
                    w.set_maximized(!w.is_maximized());
                }
            }),
        )
    }

    /// Register global window action callbacks (for drag_region() on Div).
    /// Called for both primary and secondary windows, and on focus changes.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    pub(super) fn register_window_actions_static(
        win: std::sync::Arc<winit::window::Window>,
        wake: blinc_platform_desktop::WakeProxy,
    ) {
        let d = win.clone();
        let mi = win.clone();
        let ma = win.clone();
        let cl = win;
        blinc_layout::window_actions::set_active_window_actions(
            move || {
                let _ = d.drag_window();
            },
            move || mi.set_minimized(true),
            move || ma.set_maximized(!ma.is_maximized()),
            move || {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                cl.id().hash(&mut hasher);
                wake.close_window(blinc_platform::WindowId(hasher.finish()));
            },
        );
    }

    /// Pick the wgpu `CompositeAlphaMode` to configure the surface with.
    ///
    /// Queries the surface's actual supported modes via
    /// [`wgpu::Surface::get_capabilities`] and picks the best match.
    /// Hardcoding `PostMultiplied` for non-Windows panicked on Linux
    /// Mesa, which only exposes `[Opaque, PreMultiplied]` (GH #34).
    ///
    /// Preference when `transparent` is requested:
    /// `PostMultiplied` → `PreMultiplied` → `Inherit` → `Auto` →
    /// `Opaque`. `PostMultiplied` is correct for our non-premultiplied
    /// shader output; `PreMultiplied` may slightly wash colors out
    /// against the desktop background but doesn't panic and works on
    /// Linux Mesa + Windows DWM.
    #[cfg(all(feature = "windowed", not(target_os = "android")))]
    pub(super) fn pick_alpha_mode(
        surface: &wgpu::Surface,
        adapter: &wgpu::Adapter,
        transparent: bool,
    ) -> wgpu::CompositeAlphaMode {
        if !transparent {
            return wgpu::CompositeAlphaMode::Opaque;
        }
        let supported = surface.get_capabilities(adapter).alpha_modes;
        let preference = [
            wgpu::CompositeAlphaMode::PostMultiplied,
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
            wgpu::CompositeAlphaMode::Auto,
        ];
        for mode in preference {
            if supported.contains(&mode) {
                return mode;
            }
        }
        tracing::warn!(
            "transparent window requested but the surface exposes no transparent alpha mode \
             (supported: {:?}) — falling back to Opaque",
            supported
        );
        wgpu::CompositeAlphaMode::Opaque
    }
}
