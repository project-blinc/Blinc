//! Linux GNOME Adwaita theme
//!
//! Implements the GNOME Human Interface Guidelines with libadwaita design language:
//! - Accent colors: GNOME Blue (#3584E4)
//! - Typography: Cantarell (13px base)
//! - Spacing: 6px base unit
//! - Corner radii: 6px default
//! - Characteristic Adwaita shadows

use crate::theme::{ColorScheme, Theme, ThemeBundle};
use crate::tokens::*;
use blinc_core::Color;

/// Linux-native theme inspired by GNOME Adwaita (libadwaita/GTK 4)
#[derive(Clone, Debug)]
pub struct LinuxTheme {
    scheme: ColorScheme,
    colors: ColorTokens,
    typography: TypographyTokens,
    spacing: SpacingTokens,
    radii: RadiusTokens,
    shadows: ShadowTokens,
    animations: AnimationTokens,
}

impl LinuxTheme {
    /// Create the light variant (Adwaita Light)
    pub fn light() -> Self {
        Self {
            scheme: ColorScheme::Light,
            colors: ColorTokens {
                // Primary - GNOME Accent Blue
                primary: Color::from_hex(0x3584E4),
                primary_hover: Color::from_hex(0x1C71D8),
                primary_active: Color::from_hex(0x1A63C4),

                // Secondary
                secondary: Color::from_hex(0x5E5C64),
                secondary_hover: Color::from_hex(0x504E55),
                secondary_active: Color::from_hex(0x434147),

                // Semantic Colors
                success: Color::from_hex(0x26AB2D),
                success_bg: Color::from_hex(0x26AB2D).with_alpha(0.1),
                warning: Color::from_hex(0xF57D00),
                warning_bg: Color::from_hex(0xF57D00).with_alpha(0.1),
                error: Color::from_hex(0xE01B24),
                error_bg: Color::from_hex(0xE01B24).with_alpha(0.1),
                info: Color::from_hex(0x3584E4),
                info_bg: Color::from_hex(0x3584E4).with_alpha(0.1),

                // Surfaces
                background: Color::from_hex(0xFAFAFA), // window_bg_color
                surface: Color::WHITE,                 // view_bg_color
                surface_elevated: Color::WHITE,
                surface_overlay: Color::from_hex(0xF6F5F4), // card_bg_color

                // Text
                text_primary: Color::from_hex(0x000000), // view_fg_color (pure black)
                text_secondary: Color::from_hex(0x5E5C64), // secondary_fg_color
                text_tertiary: Color::from_hex(0x9A9996), // tertiary_fg_color
                text_inverse: Color::WHITE,
                text_link: Color::from_hex(0x3584E4),

                // Borders
                border: Color::from_hex(0xCDCDCD),
                border_secondary: Color::from_hex(0xBBBBBB), // Adwaita borders_color — form controls
                border_hover: Color::from_hex(0xBBBBBB),
                border_focus: Color::from_hex(0x3584E4),
                border_error: Color::from_hex(0xE01B24),

                // Inputs
                input_bg: Color::WHITE,
                input_bg_hover: Color::from_hex(0xF8F8F8),
                input_bg_focus: Color::WHITE,
                input_bg_disabled: Color::from_hex(0xF5F5F5),

                // Selection
                selection: Color::from_hex(0x3584E4).with_alpha(0.3),
                selection_text: Color::from_hex(0x000000),

                // Accent
                accent: Color::from_hex(0x3584E4),
                accent_subtle: Color::from_hex(0x3584E4).with_alpha(0.1),
                // Tooltip (inverted for light theme)
                tooltip_bg: Color::from_hex(0x242424),
                tooltip_text: Color::from_hex(0xFAFAFA),
            },
            typography: TypographyTokens {
                font_sans: FontFamily::new(
                    "Cantarell",
                    vec!["Ubuntu", "DejaVu Sans", "system-ui", "sans-serif"],
                ),
                font_serif: FontFamily::new(
                    "DejaVu Serif",
                    vec!["Liberation Serif", "Georgia", "serif"],
                ),
                font_mono: FontFamily::new(
                    "Source Code Pro",
                    vec!["Liberation Mono", "DejaVu Sans Mono", "monospace"],
                ),
                // GNOME Typography Scale (13px base)
                text_xs: 11.0,   // Caption
                text_sm: 13.0,   // Small/Body (GNOME default)
                text_base: 13.0, // Body
                text_lg: 16.0,   // Heading 2
                text_xl: 18.0,   // Heading 1
                text_2xl: 22.0,
                text_3xl: 28.0,
                text_4xl: 34.0,
                text_5xl: 44.0,
                ..Default::default()
            },
            spacing: SpacingTokens::with_base(6.0), // Adwaita uses 6px base
            radii: RadiusTokens {
                radius_none: 0.0,
                radius_sm: 3.0,
                radius_default: 6.0, // Adwaita default
                radius_md: 6.0,      // Cards, buttons
                radius_lg: 8.0,      // Large surfaces
                radius_xl: 8.0,
                radius_2xl: 12.0,
                radius_3xl: 16.0,
                radius_full: 9999.0,
            },
            shadows: Self::light_shadows(),
            animations: AnimationTokens::default(),
        }
    }

    /// Create the dark variant (Adwaita Dark)
    pub fn dark() -> Self {
        Self {
            scheme: ColorScheme::Dark,
            colors: ColorTokens {
                // Primary - GNOME Accent Blue (consistent in dark mode)
                primary: Color::from_hex(0x3584E4),
                primary_hover: Color::from_hex(0x2878D9),
                primary_active: Color::from_hex(0x1F6FBF),

                // Secondary
                secondary: Color::from_hex(0xD0CFCC),
                secondary_hover: Color::from_hex(0xBDBCB8),
                secondary_active: Color::from_hex(0xA9A8A4),

                // Semantic Colors
                success: Color::from_hex(0x26AB2D),
                success_bg: Color::from_hex(0x26AB2D).with_alpha(0.15),
                warning: Color::from_hex(0xF57D00),
                warning_bg: Color::from_hex(0xF57D00).with_alpha(0.15),
                error: Color::from_hex(0xE01B24),
                error_bg: Color::from_hex(0xE01B24).with_alpha(0.15),
                info: Color::from_hex(0x3584E4),
                info_bg: Color::from_hex(0x3584E4).with_alpha(0.15),

                // Surfaces
                background: Color::from_hex(0x242424), // window_bg_color dark
                surface: Color::from_hex(0x1E1E1E),    // view_bg_color dark
                surface_elevated: Color::from_hex(0x2E2E2E),
                surface_overlay: Color::from_hex(0x2A2A2A), // headerbar_bg_color

                // Text
                text_primary: Color::WHITE,
                text_secondary: Color::from_hex(0xD0CFCC),
                text_tertiary: Color::from_hex(0x949390),
                text_inverse: Color::from_hex(0x000000),
                text_link: Color::from_hex(0x3584E4),

                // Borders
                border: Color::from_hex(0x3A3A3A),
                border_secondary: Color::from_hex(0x5E5E5E), // Adwaita dark borders_color — form controls
                border_hover: Color::from_hex(0x4A4A4A),
                border_focus: Color::from_hex(0x3584E4),
                border_error: Color::from_hex(0xE01B24),

                // Inputs
                input_bg: Color::from_hex(0x3A3A3A),
                input_bg_hover: Color::from_hex(0x3E3E3E),
                input_bg_focus: Color::from_hex(0x3A3A3A),
                input_bg_disabled: Color::from_hex(0x333333),

                // Selection
                selection: Color::from_hex(0x3584E4).with_alpha(0.35),
                selection_text: Color::WHITE,

                // Accent
                accent: Color::from_hex(0x3584E4),
                accent_subtle: Color::from_hex(0x3584E4).with_alpha(0.15),
                // Tooltip (inverted for dark theme)
                tooltip_bg: Color::from_hex(0xFAFAFA),
                tooltip_text: Color::from_hex(0x242424),
            },
            typography: TypographyTokens {
                font_sans: FontFamily::new(
                    "Cantarell",
                    vec!["Ubuntu", "DejaVu Sans", "system-ui", "sans-serif"],
                ),
                font_serif: FontFamily::new(
                    "DejaVu Serif",
                    vec!["Liberation Serif", "Georgia", "serif"],
                ),
                font_mono: FontFamily::new(
                    "Source Code Pro",
                    vec!["Liberation Mono", "DejaVu Sans Mono", "monospace"],
                ),
                text_xs: 11.0,
                text_sm: 13.0,
                text_base: 13.0,
                text_lg: 16.0,
                text_xl: 18.0,
                text_2xl: 22.0,
                text_3xl: 28.0,
                text_4xl: 34.0,
                text_5xl: 44.0,
                ..Default::default()
            },
            spacing: SpacingTokens::with_base(6.0),
            radii: RadiusTokens {
                radius_none: 0.0,
                radius_sm: 3.0,
                radius_default: 6.0,
                radius_md: 6.0,
                radius_lg: 8.0,
                radius_xl: 8.0,
                radius_2xl: 12.0,
                radius_3xl: 16.0,
                radius_full: 9999.0,
            },
            shadows: Self::dark_shadows(),
            animations: AnimationTokens::default(),
        }
    }

    /// Create a theme bundle with light and dark variants
    pub fn bundle() -> ThemeBundle {
        ThemeBundle::new("Linux", Self::light(), Self::dark())
    }

    /// Adwaita light mode shadows
    fn light_shadows() -> ShadowTokens {
        let base_color = Color::BLACK;
        ShadowTokens {
            shadow_sm: vec![Shadow::new(0.0, 1.0, 3.0, 0.0, base_color.with_alpha(0.12))],
            shadow_default: vec![Shadow::new(0.0, 1.0, 3.0, 0.0, base_color.with_alpha(0.12))],
            shadow_md: vec![Shadow::new(0.0, 2.0, 4.0, 0.0, base_color.with_alpha(0.15))],
            shadow_lg: vec![Shadow::new(0.0, 3.0, 6.0, 0.0, base_color.with_alpha(0.20))],
            shadow_xl: vec![Shadow::new(0.0, 4.0, 8.0, 0.0, base_color.with_alpha(0.25))],
            shadow_2xl: vec![Shadow::new(
                0.0,
                6.0,
                12.0,
                0.0,
                base_color.with_alpha(0.30),
            )],
            shadow_inner: vec![Shadow::new_inset(
                0.0,
                1.0,
                2.0,
                0.0,
                base_color.with_alpha(0.08),
            )],
            shadow_none: Vec::new(),
        }
    }

    /// Adwaita dark mode shadows (more pronounced)
    fn dark_shadows() -> ShadowTokens {
        let base_color = Color::BLACK;
        ShadowTokens {
            shadow_sm: vec![Shadow::new(0.0, 1.0, 3.0, 0.0, base_color.with_alpha(0.36))],
            shadow_default: vec![Shadow::new(0.0, 1.0, 3.0, 0.0, base_color.with_alpha(0.36))],
            shadow_md: vec![Shadow::new(0.0, 2.0, 4.0, 0.0, base_color.with_alpha(0.40))],
            shadow_lg: vec![Shadow::new(0.0, 3.0, 6.0, 0.0, base_color.with_alpha(0.50))],
            shadow_xl: vec![Shadow::new(0.0, 4.0, 8.0, 0.0, base_color.with_alpha(0.60))],
            shadow_2xl: vec![Shadow::new(
                0.0,
                6.0,
                12.0,
                0.0,
                base_color.with_alpha(0.70),
            )],
            shadow_inner: vec![Shadow::new_inset(
                0.0,
                1.0,
                2.0,
                0.0,
                base_color.with_alpha(0.20),
            )],
            shadow_none: Vec::new(),
        }
    }
}

impl Theme for LinuxTheme {
    fn name(&self) -> &str {
        "Linux"
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
