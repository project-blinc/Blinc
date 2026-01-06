//! macOS Aqua/Big Sur theme

use crate::theme::{ColorScheme, Theme, ThemeBundle};
use crate::tokens::*;
use blinc_core::Color;

/// macOS-native theme inspired by Aqua/Big Sur design language
#[derive(Clone, Debug)]
pub struct MacOSTheme {
    scheme: ColorScheme,
    colors: ColorTokens,
    typography: TypographyTokens,
    spacing: SpacingTokens,
    radii: RadiusTokens,
    shadows: ShadowTokens,
    animations: AnimationTokens,
}

impl MacOSTheme {
    /// Create the light variant
    pub fn light() -> Self {
        Self {
            scheme: ColorScheme::Light,
            colors: ColorTokens {
                // System Blue
                primary: Color::from_hex(0x007AFF),
                primary_hover: Color::from_hex(0x0056CC),
                primary_active: Color::from_hex(0x004499),
                // System Gray
                secondary: Color::from_hex(0x8E8E93),
                secondary_hover: Color::from_hex(0x6E6E73),
                secondary_active: Color::from_hex(0x5E5E63),
                // Semantic colors
                success: Color::from_hex(0x34C759),
                success_bg: Color::from_hex(0x34C759).with_alpha(0.1),
                warning: Color::from_hex(0xFF9500),
                warning_bg: Color::from_hex(0xFF9500).with_alpha(0.1),
                error: Color::from_hex(0xFF3B30),
                error_bg: Color::from_hex(0xFF3B30).with_alpha(0.1),
                info: Color::from_hex(0x5AC8FA),
                info_bg: Color::from_hex(0x5AC8FA).with_alpha(0.1),
                // Surfaces
                background: Color::from_hex(0xF5F5F7),
                surface: Color::WHITE,
                surface_elevated: Color::from_hex(0xF3F3F3),
                surface_overlay: Color::from_hex(0xE8E8ED),
                // Text
                text_primary: Color::from_hex(0x1D1D1F),
                text_secondary: Color::from_hex(0x86868B),
                text_tertiary: Color::from_hex(0xAEAEB2),
                text_inverse: Color::WHITE,
                text_link: Color::from_hex(0x007AFF),
                // Borders
                border: Color::rgba(0.0, 0.0, 0.0, 0.1),
                border_hover: Color::rgba(0.0, 0.0, 0.0, 0.15),
                border_focus: Color::from_hex(0x007AFF),
                border_error: Color::from_hex(0xFF3B30),
                // Inputs
                input_bg: Color::WHITE,
                input_bg_hover: Color::from_hex(0xFAFAFA),
                input_bg_focus: Color::WHITE,
                input_bg_disabled: Color::from_hex(0xF0F0F0),
                // Selection
                selection: Color::from_hex(0x007AFF).with_alpha(0.2),
                selection_text: Color::from_hex(0x1D1D1F),
                // Accent
                accent: Color::from_hex(0x007AFF),
                accent_subtle: Color::from_hex(0x007AFF).with_alpha(0.1),
            },
            typography: TypographyTokens {
                font_sans: FontFamily::new("SF Pro", vec!["system-ui", "-apple-system"]),
                font_serif: FontFamily::new("New York", vec!["Georgia", "serif"]),
                font_mono: FontFamily::new("SF Mono", vec!["Menlo", "monospace"]),
                ..Default::default()
            },
            spacing: SpacingTokens::default(),
            radii: RadiusTokens {
                radius_default: 6.0,
                radius_md: 8.0,
                radius_lg: 10.0,
                radius_xl: 14.0,
                ..Default::default()
            },
            shadows: ShadowTokens::light(),
            animations: AnimationTokens::default(),
        }
    }

    /// Create the dark variant
    pub fn dark() -> Self {
        Self {
            scheme: ColorScheme::Dark,
            colors: ColorTokens {
                // System Blue (Dark)
                primary: Color::from_hex(0x0A84FF),
                primary_hover: Color::from_hex(0x409CFF),
                primary_active: Color::from_hex(0x64B0FF),
                // System Gray (Dark)
                secondary: Color::from_hex(0x98989D),
                secondary_hover: Color::from_hex(0xAEAEB2),
                secondary_active: Color::from_hex(0xC7C7CC),
                // Semantic colors
                success: Color::from_hex(0x30D158),
                success_bg: Color::from_hex(0x30D158).with_alpha(0.15),
                warning: Color::from_hex(0xFF9F0A),
                warning_bg: Color::from_hex(0xFF9F0A).with_alpha(0.15),
                error: Color::from_hex(0xFF453A),
                error_bg: Color::from_hex(0xFF453A).with_alpha(0.15),
                info: Color::from_hex(0x64D2FF),
                info_bg: Color::from_hex(0x64D2FF).with_alpha(0.15),
                // Surfaces
                background: Color::from_hex(0x1E1E1E),
                surface: Color::from_hex(0x2D2D2D),
                surface_elevated: Color::from_hex(0x3A3A3A),
                surface_overlay: Color::from_hex(0x1C1C1E),
                // Text
                text_primary: Color::WHITE,
                text_secondary: Color::from_hex(0x98989D),
                text_tertiary: Color::from_hex(0x636366),
                text_inverse: Color::from_hex(0x1D1D1F),
                text_link: Color::from_hex(0x0A84FF),
                // Borders
                border: Color::rgba(1.0, 1.0, 1.0, 0.1),
                border_hover: Color::rgba(1.0, 1.0, 1.0, 0.15),
                border_focus: Color::from_hex(0x0A84FF),
                border_error: Color::from_hex(0xFF453A),
                // Inputs
                input_bg: Color::from_hex(0x2D2D2D),
                input_bg_hover: Color::from_hex(0x3A3A3A),
                input_bg_focus: Color::from_hex(0x2D2D2D),
                input_bg_disabled: Color::from_hex(0x1C1C1E),
                // Selection
                selection: Color::from_hex(0x0A84FF).with_alpha(0.3),
                selection_text: Color::WHITE,
                // Accent
                accent: Color::from_hex(0x0A84FF),
                accent_subtle: Color::from_hex(0x0A84FF).with_alpha(0.15),
            },
            typography: TypographyTokens {
                font_sans: FontFamily::new("SF Pro", vec!["system-ui", "-apple-system"]),
                font_serif: FontFamily::new("New York", vec!["Georgia", "serif"]),
                font_mono: FontFamily::new("SF Mono", vec!["Menlo", "monospace"]),
                ..Default::default()
            },
            spacing: SpacingTokens::default(),
            radii: RadiusTokens {
                radius_default: 6.0,
                radius_md: 8.0,
                radius_lg: 10.0,
                radius_xl: 14.0,
                ..Default::default()
            },
            shadows: ShadowTokens::dark(),
            animations: AnimationTokens::default(),
        }
    }

    /// Create a theme bundle with light and dark variants
    pub fn bundle() -> ThemeBundle {
        ThemeBundle::new("macOS", Self::light(), Self::dark())
    }
}

impl Theme for MacOSTheme {
    fn name(&self) -> &str {
        "macOS"
    }

    fn color_scheme(&self) -> ColorScheme {
        self.scheme
    }

    fn colors(&self) -> &ColorTokens {
        &self.colors
    }

    fn typography(&self) -> &TypographyTokens {
        &self.typography
    }

    fn spacing(&self) -> &SpacingTokens {
        &self.spacing
    }

    fn radii(&self) -> &RadiusTokens {
        &self.radii
    }

    fn shadows(&self) -> &ShadowTokens {
        &self.shadows
    }

    fn animations(&self) -> &AnimationTokens {
        &self.animations
    }
}
