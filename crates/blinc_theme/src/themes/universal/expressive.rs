//! Universal HID — **Expressive** variant (Material-leaning).
//!
//! Bolder radii, longer spring-y motion, tonal accent surfaces.
//! For consumer-facing apps that want personality — social
//! products, anything where the chrome can participate rather than
//! disappear. Subtlest squircle of the three variants (smoothing
//! 0.20, exponent 2.6) — closer to a plain circle since the radii
//! themselves are larger.

use crate::theme::{ColorScheme, Theme, ThemeBundle};
use crate::tokens::*;
use blinc_core::Color;

use super::hybrid::universal_typography;

/// Universal HID · Expressive theme.
#[derive(Clone, Debug)]
pub struct ExpressiveTheme {
    scheme: ColorScheme,
    colors: ColorTokens,
    typography: TypographyTokens,
    spacing: SpacingTokens,
    radii: RadiusTokens,
    shape: ShapeTokens,
    shadows: ShadowTokens,
    animations: AnimationTokens,
}

impl ExpressiveTheme {
    /// Expressive · Light.
    pub fn light() -> Self {
        Self {
            scheme: ColorScheme::Light,
            colors: ColorTokens {
                primary: Color::from_hex(0x3D5FE8),
                primary_hover: Color::from_hex(0x314DD0),
                primary_active: Color::from_hex(0x253BB0),
                secondary: Color::from_hex(0x665E7E),
                secondary_hover: Color::from_hex(0x504864),
                secondary_active: Color::from_hex(0x3A344A),
                success: Color::from_hex(0x1E8B49),
                success_bg: Color::from_hex(0xD9F2E0),
                warning: Color::from_hex(0xA06A12),
                warning_bg: Color::from_hex(0xF8E6C0),
                error: Color::from_hex(0xB53A30),
                error_bg: Color::from_hex(0xFADAD5),
                info: Color::from_hex(0x0E7AB5),
                info_bg: Color::from_hex(0xD5ECF6),
                // Surfaces carry an accent tint — Material's tonal scheme.
                background: Color::from_hex(0xFBFAFE),
                surface: Color::WHITE,
                surface_elevated: Color::from_hex(0xF4F4FE),
                surface_overlay: Color::from_hex(0xE5EBFE),
                text_primary: Color::from_hex(0x161726),
                text_secondary: Color::from_hex(0x4B4F66),
                text_tertiary: Color::from_hex(0x84899E),
                text_inverse: Color::WHITE,
                text_link: Color::from_hex(0x3D5FE8),
                border: Color::from_hex(0x161726).with_alpha(0.10),
                border_secondary: Color::from_hex(0xD2D5E5),
                border_hover: Color::from_hex(0x161726).with_alpha(0.18),
                border_focus: Color::from_hex(0x3D5FE8),
                border_error: Color::from_hex(0xB53A30),
                input_bg: Color::WHITE,
                input_bg_hover: Color::from_hex(0xF8F8FE),
                input_bg_focus: Color::WHITE,
                input_bg_disabled: Color::from_hex(0xECEDF8),
                selection: Color::from_hex(0x3D5FE8).with_alpha(0.24),
                selection_text: Color::from_hex(0x161726),
                accent: Color::from_hex(0x3D5FE8),
                accent_subtle: Color::from_hex(0xDCE3FF),
                tooltip_bg: Color::from_hex(0x1A1B2B),
                tooltip_text: Color::from_hex(0xFBFAFE),
            },
            typography: universal_typography(15.0),
            spacing: SpacingTokens::default(),
            radii: RadiusTokens {
                // Bolder ladder throughout.
                radius_none: 0.0,
                radius_sm: 4.0,
                // Sits at `smoothing_threshold` (16.0) so default
                // corners engage the Expressive squircle. Below the
                // threshold every corner falls back to a true
                // circle, which left buttons / inputs (the most
                // common surfaces consuming `--radius-default`) on
                // the circular fallback. `radius_md` (12.0) stays
                // smaller — default > md is intentional for this
                // token: `default` is "what unmarked surfaces get",
                // not a strict ladder step.
                radius_default: 16.0,
                radius_md: 12.0,
                radius_lg: 16.0,
                radius_xl: 24.0,
                radius_2xl: 28.0,
                radius_3xl: 36.0,
                radius_full: 9999.0,
            },
            shape: ShapeTokens {
                corner_smoothing: 0.20,
                corner_exponent: 2.6,
                smoothing_threshold: 16.0,
            },
            shadows: expressive_shadows_light(),
            animations: AnimationTokens {
                // Emphasised easing throughout — Material's signature.
                // Longer durations.
                duration_fastest: 100,
                duration_faster: 150,
                duration_fast: 200,
                duration_normal: 280,
                duration_slow: 400,
                duration_slower: 500,
                duration_slowest: 650,
                ease_default: EMPH_DECEL,
                ease_in: Easing::EaseIn,
                ease_out: EMPH_DECEL,
                ease_in_out: EMPHASIZED,
                // Material 3-leaning semantic slots — emphasised
                // curves throughout, springier popovers.
                ease_state: EMPH_DECEL,
                ease_nav: EMPHASIZED,
                ease_spring: EXPRESSIVE_SPRING,
                ease_sheet: EMPHASIZED,
            },
        }
    }

    /// Expressive · Dark.
    pub fn dark() -> Self {
        Self {
            scheme: ColorScheme::Dark,
            colors: ColorTokens {
                primary: Color::from_hex(0xB1C5FF),
                primary_hover: Color::from_hex(0xC4D2FF),
                primary_active: Color::from_hex(0xD7E0FF),
                secondary: Color::from_hex(0xCFC8E2),
                secondary_hover: Color::from_hex(0xE2DCEF),
                secondary_active: Color::from_hex(0xF0ECF8),
                success: Color::from_hex(0x5EDB89),
                success_bg: Color::from_hex(0x5EDB89).with_alpha(0.18),
                warning: Color::from_hex(0xF2BD60),
                warning_bg: Color::from_hex(0xF2BD60).with_alpha(0.18),
                error: Color::from_hex(0xFF7F73),
                error_bg: Color::from_hex(0xFF7F73).with_alpha(0.18),
                info: Color::from_hex(0x82C9F0),
                info_bg: Color::from_hex(0x82C9F0).with_alpha(0.18),
                background: Color::from_hex(0x11121A),
                surface: Color::from_hex(0x1B1D29),
                surface_elevated: Color::from_hex(0x262838),
                surface_overlay: Color::from_hex(0x0A0B12),
                text_primary: Color::from_hex(0xE4E5F0),
                text_secondary: Color::from_hex(0xB6BACA),
                text_tertiary: Color::from_hex(0x7B8094),
                text_inverse: Color::from_hex(0x11121A),
                text_link: Color::from_hex(0xB1C5FF),
                border: Color::WHITE.with_alpha(0.10),
                border_secondary: Color::from_hex(0x3D4055),
                border_hover: Color::WHITE.with_alpha(0.20),
                border_focus: Color::from_hex(0xB1C5FF),
                border_error: Color::from_hex(0xFF7F73),
                input_bg: Color::from_hex(0x1B1D29),
                input_bg_hover: Color::from_hex(0x242636),
                input_bg_focus: Color::from_hex(0x1B1D29),
                input_bg_disabled: Color::from_hex(0x14151E),
                selection: Color::from_hex(0xB1C5FF).with_alpha(0.32),
                selection_text: Color::from_hex(0xE4E5F0),
                accent: Color::from_hex(0xB1C5FF),
                accent_subtle: Color::from_hex(0xB1C5FF).with_alpha(0.18),
                tooltip_bg: Color::from_hex(0xE4E5F0),
                tooltip_text: Color::from_hex(0x11121A),
            },
            typography: universal_typography(15.0),
            spacing: SpacingTokens::default(),
            radii: RadiusTokens {
                radius_none: 0.0,
                radius_sm: 4.0,
                // Sits at `smoothing_threshold` (16.0) so default
                // corners engage the Expressive squircle. Below the
                // threshold every corner falls back to a true
                // circle, which left buttons / inputs (the most
                // common surfaces consuming `--radius-default`) on
                // the circular fallback. `radius_md` (12.0) stays
                // smaller — default > md is intentional for this
                // token: `default` is "what unmarked surfaces get",
                // not a strict ladder step.
                radius_default: 16.0,
                radius_md: 12.0,
                radius_lg: 16.0,
                radius_xl: 24.0,
                radius_2xl: 28.0,
                radius_3xl: 36.0,
                radius_full: 9999.0,
            },
            shape: ShapeTokens {
                corner_smoothing: 0.20,
                corner_exponent: 2.6,
                smoothing_threshold: 16.0,
            },
            shadows: expressive_shadows_dark(),
            animations: AnimationTokens {
                duration_fastest: 100,
                duration_faster: 150,
                duration_fast: 200,
                duration_normal: 280,
                duration_slow: 400,
                duration_slower: 500,
                duration_slowest: 650,
                ease_default: EMPH_DECEL,
                ease_in: Easing::EaseIn,
                ease_out: EMPH_DECEL,
                ease_in_out: EMPHASIZED,
                // Material 3-leaning semantic slots — emphasised
                // curves throughout, springier popovers.
                ease_state: EMPH_DECEL,
                ease_nav: EMPHASIZED,
                ease_spring: EXPRESSIVE_SPRING,
                ease_sheet: EMPHASIZED,
            },
        }
    }

    /// Bundle the light + dark variants.
    pub fn bundle() -> ThemeBundle {
        ThemeBundle::new("Universal · Expressive", Self::light(), Self::dark())
    }
}

impl Theme for ExpressiveTheme {
    fn name(&self) -> &str {
        "Universal · Expressive"
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
    fn shape(&self) -> &ShapeTokens {
        &self.shape
    }
    fn shadows(&self) -> &ShadowTokens {
        &self.shadows
    }
    fn animations(&self) -> &AnimationTokens {
        &self.animations
    }
}

// Material 3 "emphasized" — slow start, fast finish.
const EMPHASIZED: Easing = Easing::CubicBezier(0.20, 0.00, 0.00, 1.00);
// Material 3 emphasized-decelerate for incoming elements.
const EMPH_DECEL: Easing = Easing::CubicBezier(0.05, 0.70, 0.10, 1.00);

// Expressive popover/spring — pronounced overshoot (~10% past target).
const EXPRESSIVE_SPRING: Easing = Easing::CubicBezier(0.34, 1.56, 0.64, 1.0);

// Expressive uses triple-layer shadows with a hint of accent tint —
// Material's signature compound depth recipe. Inner tight ambient +
// mid contact + wide directional key.
fn expressive_shadows_light() -> ShadowTokens {
    let ink = Color::from_hex(0x161726);
    let accent = Color::from_hex(0x3D5FE8);
    ShadowTokens {
        shadow_sm: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.04)),
            Shadow::new(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.06)),
        ],
        shadow_default: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.04)),
            Shadow::new(0.0, 1.0, 3.0, 0.0, ink.with_alpha(0.06)),
            Shadow::new(0.0, 2.0, 6.0, 0.0, accent.with_alpha(0.04)),
        ],
        shadow_md: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.05)),
            Shadow::new(0.0, 2.0, 4.0, 0.0, ink.with_alpha(0.06)),
            Shadow::new(0.0, 4.0, 10.0, 0.0, accent.with_alpha(0.05)),
        ],
        shadow_lg: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.05)),
            Shadow::new(0.0, 6.0, 12.0, 0.0, ink.with_alpha(0.07)),
            Shadow::new(0.0, 14.0, 28.0, 0.0, accent.with_alpha(0.06)),
        ],
        shadow_xl: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.06)),
            Shadow::new(0.0, 12.0, 22.0, 0.0, ink.with_alpha(0.08)),
            Shadow::new(0.0, 28.0, 48.0, 0.0, accent.with_alpha(0.08)),
        ],
        shadow_2xl: vec![
            Shadow::new(0.0, 0.0, 2.0, 0.0, ink.with_alpha(0.08)),
            Shadow::new(0.0, 22.0, 38.0, 0.0, ink.with_alpha(0.10)),
            Shadow::new(0.0, 48.0, 80.0, 0.0, accent.with_alpha(0.10)),
        ],
        shadow_inner: vec![Shadow::new_inset(0.0, 2.0, 4.0, 0.0, ink.with_alpha(0.08))],
        shadow_none: Vec::new(),
    }
}

fn expressive_shadows_dark() -> ShadowTokens {
    let ink = Color::BLACK;
    let accent = Color::from_hex(0x3D5FE8);
    ShadowTokens {
        shadow_sm: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.32)),
            Shadow::new(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.40)),
        ],
        shadow_default: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.32)),
            Shadow::new(0.0, 1.0, 3.0, 0.0, ink.with_alpha(0.40)),
            Shadow::new(0.0, 2.0, 6.0, 0.0, accent.with_alpha(0.12)),
        ],
        shadow_md: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.36)),
            Shadow::new(0.0, 2.0, 4.0, 0.0, ink.with_alpha(0.42)),
            Shadow::new(0.0, 4.0, 10.0, 0.0, accent.with_alpha(0.14)),
        ],
        shadow_lg: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.40)),
            Shadow::new(0.0, 6.0, 12.0, 0.0, ink.with_alpha(0.46)),
            Shadow::new(0.0, 14.0, 28.0, 0.0, accent.with_alpha(0.16)),
        ],
        shadow_xl: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.44)),
            Shadow::new(0.0, 12.0, 22.0, 0.0, ink.with_alpha(0.52)),
            Shadow::new(0.0, 28.0, 48.0, 0.0, accent.with_alpha(0.18)),
        ],
        shadow_2xl: vec![
            Shadow::new(0.0, 0.0, 2.0, 0.0, ink.with_alpha(0.50)),
            Shadow::new(0.0, 22.0, 38.0, 0.0, ink.with_alpha(0.60)),
            Shadow::new(0.0, 48.0, 80.0, 0.0, accent.with_alpha(0.22)),
        ],
        shadow_inner: vec![Shadow::new_inset(0.0, 2.0, 4.0, 0.0, ink.with_alpha(0.35))],
        shadow_none: Vec::new(),
    }
}
