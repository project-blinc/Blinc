//! Default theme bundle for blinc_cn components.
//!
//! `cn_bundle()` forwards to the framework's
//! [`blinc_theme::platform_theme_bundle`] (so cn components follow the
//! host OS aesthetic) and attaches [`crate::cn_styles::CN_STYLES`] via
//! [`blinc_theme::ThemeBundle::with_css`] so the default component
//! stylesheet ships in the bundle itself — callers don't have to
//! register it by hand.
//!
//! ```ignore
//! use blinc_app::prelude::*;
//! use blinc_cn::cn_bundle;
//!
//! WindowedApp::run_with_theme(
//!     WindowConfig::default(),
//!     cn_bundle(),
//!     ColorScheme::Dark,
//!     my_ui,
//! )
//! ```
//!
//! Chain additional `with_css` / `with_css_file` calls to layer your
//! own overrides on top:
//!
//! ```ignore
//! let bundle = blinc_cn::cn_bundle()
//!     .with_css(r#" .cn-card { border-radius: 0; } "#)
//!     .with_css_file("./styles/overrides.css");
//! ```
//!
//! When you need a different aesthetic entirely, build your own
//! [`blinc_theme::ThemeBundle`] and pass it to `run_with_theme` /
//! `ThemeState::init` directly.

use blinc_theme::{ThemeBundle, platform_theme_bundle};

/// The default theme bundle paired with blinc_cn's CSS.
///
/// Returns the framework's platform-detected bundle pre-loaded with
/// [`crate::cn_styles::CN_STYLES`] and the code editor's
/// [`blinc_layout::widgets::CODE_EDITOR_STYLES`]. Chain `with_css` /
/// `with_css_file` on the returned value to layer additional stylesheets.
pub fn cn_bundle() -> ThemeBundle {
    platform_theme_bundle()
        .with_css(crate::cn_styles::CN_STYLES)
        .with_css(blinc_layout::widgets::CODE_EDITOR_STYLES)
}
