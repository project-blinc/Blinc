//! Blinc Theme System
//!
//! A comprehensive theming system with design tokens, platform-native themes,
//! and system dark/light mode detection.
//!
//! # Overview
//!
//! The theme system provides:
//! - **Design tokens**: Colors, typography, spacing, radii, shadows, animations
//! - **Platform themes**: Native look and feel for macOS, Windows and Linux; iOS and Android use Blinc's Expressive
//! - **Color scheme detection**: Automatic detection of system dark/light mode
//! - **Dynamic overrides**: Runtime customization without layout rebuilds
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use blinc_theme::{ThemeState, ColorToken};
//!
//! // Initialize theme at app startup
//! ThemeState::init_default();
//!
//! // Access theme in widgets
//! let theme = ThemeState::get();
//! let primary_color = theme.color(ColorToken::Primary);
//! let spacing = theme.spacing();
//! ```
//!
//! # Architecture
//!
//! The theme system is designed to minimize UI rebuilds:
//!
//! - **Visual tokens** (colors, shadows): Changes trigger repaint only
//! - **Layout tokens** (spacing, typography): Changes trigger partial layout
//!
//! # Tokens
//!
//! Tokens are the atomic values that make up the design system:
//!
//! - [`ColorTokens`]: Semantic colors (primary, error, background, text, etc.)
//! - [`TypographyTokens`]: Font families, sizes, weights, line heights
//! - [`SpacingTokens`]: 4px-based spacing scale
//! - [`RadiusTokens`]: Border radii
//! - [`ShadowTokens`]: Box shadows
//! - [`AnimationTokens`]: Durations and easings
//!
//! # Themes
//!
//! Built-in themes:
//!
//! - [`BlincTheme`]: Default theme derived from Catppuccin design system
//! - Platform-specific themes for macOS, Windows and Linux
//!
//! # Dynamic Overrides
//!
//! Override tokens at runtime without full rebuilds:
//!
//! ```rust,ignore
//! let theme = ThemeState::get();
//!
//! // Override a color (repaint only, no layout)
//! theme.set_color_override(ColorToken::Primary, Color::from_hex(0xFF5500));
//!
//! // Override spacing (triggers partial layout)
//! theme.set_spacing_override(SpacingToken::Space4, 20.0);
//!
//! // Clear all overrides
//! theme.clear_overrides();
//! ```

pub mod platform;
pub mod state;
pub mod theme;
pub mod themes;
pub mod tokens;

#[cfg(feature = "watcher")]
pub mod watcher;

// Re-export commonly used types
pub use platform::{Platform, detect_system_color_scheme};
pub use state::{ThemeState, set_redraw_callback};
pub use theme::{ColorScheme, Theme, ThemeBundle};
pub use themes::{
    BlincTheme, DefaultTheme, ExpressiveTheme, HybridTheme, RestrainedTheme,
    platform::platform_theme_bundle,
};
pub use tokens::*;

#[cfg(feature = "watcher")]
pub use watcher::{SystemSchemeWatcher, WatcherConfig};
