//! Windows Fluent Design 2 theme
//!
//! Implements the Windows 11 Fluent Design System 2 design language with:
//! - Accent colors: Windows Blue (#0078D4 light, #60CDFF dark)
//! - Typography: Segoe UI Variable
//! - Corner radii: 4px (controls), 8px (flyouts/dialogs)
//! - Subtle shadows with Acrylic-style elevation

use crate::theme::{ColorScheme, Theme, ThemeBundle};
use crate::tokens::*;
use blinc_core::Color;

/// Windows-native theme inspired by Fluent Design System 2 (Windows 11)
#[derive(Clone, Debug)]
pub struct WindowsTheme {
    scheme: ColorScheme,
    colors: ColorTokens,
    typography: TypographyTokens,
    spacing: SpacingTokens,
    radii: RadiusTokens,
    shadows: ShadowTokens,
    animations: AnimationTokens,
}

impl WindowsTheme {
    /// Create the light variant (Windows 11 Light Mode)
    pub fn light() -> Self {
        Self {
            scheme: ColorScheme::Light,
            colors: ColorTokens {
                // Primary - Windows Accent Blue
                primary: Color::from_hex(0x0078D4),
                primary_hover: Color::from_hex(0x106EBE),
                primary_active: Color::from_hex(0x005A9E),

                // Secondary - Neutral Gray
                secondary: Color::from_hex(0x8A8886),
                secondary_hover: Color::from_hex(0x797775),
                secondary_active: Color::from_hex(0x605E5C),

                // Semantic Colors
                success: Color::from_hex(0x107C10), // Fluent Green
                success_bg: Color::from_hex(0x107C10).with_alpha(0.1),
                warning: Color::from_hex(0xFFB900), // Fluent Yellow
                warning_bg: Color::from_hex(0xFFB900).with_alpha(0.1),
                error: Color::from_hex(0xD13438), // Fluent Red
                error_bg: Color::from_hex(0xD13438).with_alpha(0.1),
                info: Color::from_hex(0x0078D4), // Same as primary
                info_bg: Color::from_hex(0x0078D4).with_alpha(0.1),

                // Surfaces
                background: Color::from_hex(0xF3F3F3), // SystemChromeMediumLow
                surface: Color::WHITE,                 // Card background
                surface_elevated: Color::WHITE,
                surface_overlay: Color::from_hex(0xE1DFDD), // AcrylicBackgroundFill

                // Text
                text_primary: Color::from_hex(0x1A1A1A), // Near black
                text_secondary: Color::from_hex(0x616161), // 75% opacity equiv
                text_tertiary: Color::from_hex(0x8A8A8A), // 60% opacity equiv
                text_inverse: Color::WHITE,
                text_link: Color::from_hex(0x0078D4),

                // Borders
                border: Color::rgba(0.0, 0.0, 0.0, 0.08), // CardStrokeColorDefault
                border_secondary: Color::from_hex(0xBCBCBC), // Fluent ControlStrokeColor — form controls
                border_hover: Color::rgba(0.0, 0.0, 0.0, 0.12),
                border_focus: Color::from_hex(0x0078D4), // 2px accent
                border_error: Color::from_hex(0xD13438),

                // Inputs
                input_bg: Color::WHITE,
                input_bg_hover: Color::from_hex(0xF9F9F9),
                input_bg_focus: Color::WHITE,
                input_bg_disabled: Color::from_hex(0xF3F3F3),

                // Selection
                selection: Color::from_hex(0x0078D4).with_alpha(0.3),
                selection_text: Color::from_hex(0x1A1A1A),

                // Accent
                accent: Color::from_hex(0x0078D4),
                accent_subtle: Color::from_hex(0x0078D4).with_alpha(0.1),
                // Tooltip (inverted for light theme)
                tooltip_bg: Color::from_hex(0x2D2D2D),
                tooltip_text: Color::from_hex(0xF3F3F3),
            },
            typography: TypographyTokens {
                font_sans: FontFamily::new(
                    "Segoe UI Variable",
                    vec!["Segoe UI", "system-ui", "sans-serif"],
                ),
                font_serif: FontFamily::new("Cambria", vec!["Georgia", "serif"]),
                font_mono: FontFamily::new(
                    "Cascadia Code",
                    vec!["Cascadia Mono", "Consolas", "monospace"],
                ),
                // Windows 11 Type Ramp
                text_xs: 12.0,   // Caption
                text_sm: 14.0,   // Body
                text_base: 14.0, // Body (Windows default is 14px)
                text_lg: 18.0,   // Body Large
                text_xl: 20.0,   // Subtitle
                text_2xl: 28.0,  // Title
                text_3xl: 40.0,  // Title Large
                text_4xl: 48.0,
                text_5xl: 68.0, // Display
                ..Default::default()
            },
            spacing: SpacingTokens::default(), // Windows uses 40epx grid, 4px base works well
            radii: RadiusTokens {
                radius_none: 0.0,
                radius_sm: 2.0,
                radius_default: 4.0, // Standard control radius
                radius_md: 4.0,      // Buttons, TextBox, ComboBox
                radius_lg: 8.0,      // Flyout, Dialog, Tooltips
                radius_xl: 8.0,      // ContentDialog, TabView
                radius_2xl: 8.0,
                radius_3xl: 8.0,
                radius_full: 9999.0,
            },
            shadows: Self::light_shadows(),
            animations: AnimationTokens::default(),
        }
    }

    /// Create the dark variant (Windows 11 Dark Mode)
    pub fn dark() -> Self {
        Self {
            scheme: ColorScheme::Dark,
            colors: ColorTokens {
                // Primary - Windows Accent Blue (lighter for dark mode)
                primary: Color::from_hex(0x60CDFF),
                primary_hover: Color::from_hex(0x98D8FF),
                primary_active: Color::from_hex(0x4CC2FF),

                // Secondary
                secondary: Color::from_hex(0x9E9E9E),
                secondary_hover: Color::from_hex(0xABABAB),
                secondary_active: Color::from_hex(0xBDBDBD),

                // Semantic Colors (adjusted for dark)
                success: Color::from_hex(0x6CCB5F),
                success_bg: Color::from_hex(0x6CCB5F).with_alpha(0.15),
                warning: Color::from_hex(0xFCE100),
                warning_bg: Color::from_hex(0xFCE100).with_alpha(0.15),
                error: Color::from_hex(0xFF99A4),
                error_bg: Color::from_hex(0xFF99A4).with_alpha(0.15),
                info: Color::from_hex(0x60CDFF),
                info_bg: Color::from_hex(0x60CDFF).with_alpha(0.15),

                // Surfaces
                background: Color::from_hex(0x202020), // SystemChromeMediumLow Dark
                surface: Color::from_hex(0x2D2D2D),    // Card dark
                surface_elevated: Color::from_hex(0x383838),
                surface_overlay: Color::from_hex(0x1F1F1F),

                // Text
                text_primary: Color::WHITE,
                text_secondary: Color::from_hex(0xC5C5C5),
                text_tertiary: Color::from_hex(0x9A9A9A),
                text_inverse: Color::from_hex(0x1A1A1A),
                text_link: Color::from_hex(0x60CDFF),

                // Borders
                border: Color::rgba(1.0, 1.0, 1.0, 0.08),
                border_secondary: Color::from_hex(0x6B6B6B), // Fluent ControlStrokeColor dark — form controls
                border_hover: Color::rgba(1.0, 1.0, 1.0, 0.12),
                border_focus: Color::from_hex(0x60CDFF),
                border_error: Color::from_hex(0xFF99A4),

                // Inputs
                input_bg: Color::from_hex(0x2D2D2D),
                input_bg_hover: Color::from_hex(0x383838),
                input_bg_focus: Color::from_hex(0x2D2D2D),
                input_bg_disabled: Color::from_hex(0x1F1F1F),

                // Selection
                selection: Color::from_hex(0x60CDFF).with_alpha(0.3),
                selection_text: Color::WHITE,

                // Accent
                accent: Color::from_hex(0x60CDFF),
                accent_subtle: Color::from_hex(0x60CDFF).with_alpha(0.15),
                // Tooltip (inverted for dark theme)
                tooltip_bg: Color::from_hex(0xF3F3F3),
                tooltip_text: Color::from_hex(0x2D2D2D),
            },
            typography: TypographyTokens {
                font_sans: FontFamily::new(
                    "Segoe UI Variable",
                    vec!["Segoe UI", "system-ui", "sans-serif"],
                ),
                font_serif: FontFamily::new("Cambria", vec!["Georgia", "serif"]),
                font_mono: FontFamily::new(
                    "Cascadia Code",
                    vec!["Cascadia Mono", "Consolas", "monospace"],
                ),
                text_xs: 12.0,
                text_sm: 14.0,
                text_base: 14.0,
                text_lg: 18.0,
                text_xl: 20.0,
                text_2xl: 28.0,
                text_3xl: 40.0,
                text_4xl: 48.0,
                text_5xl: 68.0,
                ..Default::default()
            },
            spacing: SpacingTokens::default(),
            radii: RadiusTokens {
                radius_none: 0.0,
                radius_sm: 2.0,
                radius_default: 4.0,
                radius_md: 4.0,
                radius_lg: 8.0,
                radius_xl: 8.0,
                radius_2xl: 8.0,
                radius_3xl: 8.0,
                radius_full: 9999.0,
            },
            shadows: Self::dark_shadows(),
            animations: AnimationTokens::default(),
        }
    }

    /// Create a theme bundle with light and dark variants
    pub fn bundle() -> ThemeBundle {
        ThemeBundle::new("Windows", Self::light(), Self::dark())
    }

    /// Windows uses subtle shadows - Acrylic provides most elevation
    fn light_shadows() -> ShadowTokens {
        let base_color = Color::BLACK;
        ShadowTokens {
            shadow_sm: vec![Shadow::new(0.0, 1.0, 2.0, 0.0, base_color.with_alpha(0.04))],
            shadow_default: vec![Shadow::new(0.0, 2.0, 4.0, 0.0, base_color.with_alpha(0.06))],
            shadow_md: vec![Shadow::new(0.0, 4.0, 8.0, 0.0, base_color.with_alpha(0.08))],
            shadow_lg: vec![Shadow::new(0.0, 8.0, 16.0, 0.0, base_color.with_alpha(0.1))],
            shadow_xl: vec![Shadow::new(
                0.0,
                16.0,
                24.0,
                0.0,
                base_color.with_alpha(0.12),
            )],
            shadow_2xl: vec![Shadow::new(
                0.0,
                24.0,
                48.0,
                0.0,
                base_color.with_alpha(0.16),
            )],
            shadow_inner: vec![Shadow::new_inset(
                0.0,
                1.0,
                2.0,
                0.0,
                base_color.with_alpha(0.04),
            )],
            shadow_none: Vec::new(),
        }
    }

    fn dark_shadows() -> ShadowTokens {
        let base_color = Color::BLACK;
        ShadowTokens {
            shadow_sm: vec![Shadow::new(0.0, 1.0, 2.0, 0.0, base_color.with_alpha(0.16))],
            shadow_default: vec![Shadow::new(0.0, 2.0, 4.0, 0.0, base_color.with_alpha(0.24))],
            shadow_md: vec![Shadow::new(0.0, 4.0, 8.0, 0.0, base_color.with_alpha(0.28))],
            shadow_lg: vec![Shadow::new(
                0.0,
                8.0,
                16.0,
                0.0,
                base_color.with_alpha(0.32),
            )],
            shadow_xl: vec![Shadow::new(
                0.0,
                16.0,
                24.0,
                0.0,
                base_color.with_alpha(0.36),
            )],
            shadow_2xl: vec![Shadow::new(
                0.0,
                24.0,
                48.0,
                0.0,
                base_color.with_alpha(0.44),
            )],
            shadow_inner: vec![Shadow::new_inset(
                0.0,
                1.0,
                2.0,
                0.0,
                base_color.with_alpha(0.12),
            )],
            shadow_none: Vec::new(),
        }
    }
}

impl Theme for WindowsTheme {
    fn name(&self) -> &str {
        "Windows"
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
