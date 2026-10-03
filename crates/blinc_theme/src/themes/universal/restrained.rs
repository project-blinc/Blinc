//! Universal HID — **Restrained** variant (Apple-leaning).
//!
//! Quiet motion, tight curves, single-layer ambient shadows. For
//! productivity tools, IDE-class apps, dashboards — anything that
//! needs to fade into the background while the user works. Most
//! pronounced squircle of the three variants (smoothing 0.65,
//! exponent 4.4).

use crate::theme::{ColorScheme, Theme, ThemeBundle};
use crate::tokens::*;
use blinc_core::Color;

use super::hybrid::universal_typography;

/// Universal HID · Restrained theme.
#[derive(Clone, Debug)]
pub struct RestrainedTheme {
    scheme: ColorScheme,
    colors: ColorTokens,
    typography: TypographyTokens,
    spacing: SpacingTokens,
    radii: RadiusTokens,
    shape: ShapeTokens,
    shadows: ShadowTokens,
    animations: AnimationTokens,
}

impl RestrainedTheme {
    /// Restrained · Light.
    pub fn light() -> Self {
        Self {
            scheme: ColorScheme::Light,
            colors: ColorTokens {
                primary: Color::from_hex(0x0A6CF0),
                primary_hover: Color::from_hex(0x0858CC),
                primary_active: Color::from_hex(0x0A47A8),
                secondary: Color::from_hex(0x5E5E66),
                secondary_hover: Color::from_hex(0x46464C),
                secondary_active: Color::from_hex(0x36363B),
                success: Color::from_hex(0x1F9D55),
                success_bg: Color::from_hex(0x1F9D55).with_alpha(0.10),
                warning: Color::from_hex(0xC28200),
                warning_bg: Color::from_hex(0xC28200).with_alpha(0.10),
                error: Color::from_hex(0xD8392C),
                error_bg: Color::from_hex(0xD8392C).with_alpha(0.10),
                info: Color::from_hex(0x1991DB),
                info_bg: Color::from_hex(0x1991DB).with_alpha(0.10),
                background: Color::from_hex(0xF5F6F8),
                surface: Color::WHITE,
                surface_elevated: Color::WHITE,
                surface_overlay: Color::from_hex(0xECEEF2),
                text_primary: Color::from_hex(0x14181F),
                text_secondary: Color::from_hex(0x5B6271),
                text_tertiary: Color::from_hex(0x9098A6),
                text_inverse: Color::WHITE,
                text_link: Color::from_hex(0x0A6CF0),
                border: Color::from_hex(0x14181F).with_alpha(0.10),
                border_secondary: Color::from_hex(0xD5D9E0),
                border_hover: Color::from_hex(0x14181F).with_alpha(0.16),
                border_focus: Color::from_hex(0x0A6CF0),
                border_error: Color::from_hex(0xD8392C),
                input_bg: Color::WHITE,
                input_bg_hover: Color::from_hex(0xFAFBFC),
                input_bg_focus: Color::WHITE,
                input_bg_disabled: Color::from_hex(0xF0F1F4),
                selection: Color::from_hex(0x0A6CF0).with_alpha(0.22),
                selection_text: Color::from_hex(0x14181F),
                accent: Color::from_hex(0x0A6CF0),
                accent_subtle: Color::from_hex(0x0A6CF0).with_alpha(0.10),
                tooltip_bg: Color::from_hex(0x1C1F26),
                tooltip_text: Color::from_hex(0xF5F6F8),
            },
            typography: universal_typography(15.0),
            spacing: SpacingTokens::default(),
            radii: RadiusTokens {
                radius_none: 0.0,
                radius_sm: 3.0,
                // Sits at `smoothing_threshold` (12.0) so default
                // corners engage the Restrained squircle. Below the
                // threshold every corner falls back to a true circle
                // (see `resolve_corner_shape`) and the variant's
                // signature shape never reaches the most common
                // surfaces (buttons, inputs). `radius_md` (8.0)
                // stays smaller — default > md is intentional for
                // this token: `default` is "what unmarked surfaces
                // get", not a strict ladder step.
                radius_default: 12.0,
                radius_md: 8.0,
                radius_lg: 10.0,
                radius_xl: 14.0,
                radius_2xl: 18.0,
                radius_3xl: 24.0,
                radius_full: 9999.0,
            },
            shape: ShapeTokens {
                corner_smoothing: 0.65,
                corner_exponent: 4.4,
                smoothing_threshold: 12.0,
            },
            shadows: restrained_shadows_light(),
            animations: AnimationTokens {
                // Single curve everywhere — Apple's signature
                // restraint. Tighter duration band.
                duration_fastest: 75,
                duration_faster: 100,
                duration_fast: 150,
                duration_normal: 200,
                duration_slow: 280,
                duration_slower: 360,
                duration_slowest: 460,
                ease_default: STANDARD,
                ease_in: Easing::EaseIn,
                ease_out: STANDARD,
                ease_in_out: Easing::EaseInOut,
                // Apple HIG-leaning semantic slots: snappy state
                // feedback, long-tail navigation, light spring.
                ease_state: STANDARD,
                ease_nav: HIG_SHEET,
                ease_spring: SUBTLE_SPRING,
                ease_sheet: HIG_SHEET,
            },
        }
    }

    /// Restrained · Dark.
    pub fn dark() -> Self {
        Self {
            scheme: ColorScheme::Dark,
            colors: ColorTokens {
                primary: Color::from_hex(0x4F94FF),
                primary_hover: Color::from_hex(0x6FA8FF),
                primary_active: Color::from_hex(0x8FBCFF),
                secondary: Color::from_hex(0xA6A6AD),
                secondary_hover: Color::from_hex(0xBFBFC4),
                secondary_active: Color::from_hex(0xD8D8DB),
                success: Color::from_hex(0x3DCB7B),
                success_bg: Color::from_hex(0x3DCB7B).with_alpha(0.14),
                warning: Color::from_hex(0xE8A93B),
                warning_bg: Color::from_hex(0xE8A93B).with_alpha(0.14),
                error: Color::from_hex(0xFF6258),
                error_bg: Color::from_hex(0xFF6258).with_alpha(0.14),
                info: Color::from_hex(0x5BB6E8),
                info_bg: Color::from_hex(0x5BB6E8).with_alpha(0.14),
                background: Color::from_hex(0x0F1216),
                surface: Color::from_hex(0x181C22),
                surface_elevated: Color::from_hex(0x232830),
                surface_overlay: Color::from_hex(0x0B0D11),
                text_primary: Color::from_hex(0xF2F4F7),
                text_secondary: Color::from_hex(0xA9B0BD),
                text_tertiary: Color::from_hex(0x717886),
                text_inverse: Color::from_hex(0x0F1216),
                text_link: Color::from_hex(0x4F94FF),
                border: Color::WHITE.with_alpha(0.10),
                border_secondary: Color::from_hex(0x353A45),
                border_hover: Color::WHITE.with_alpha(0.16),
                border_focus: Color::from_hex(0x4F94FF),
                border_error: Color::from_hex(0xFF6258),
                input_bg: Color::from_hex(0x181C22),
                input_bg_hover: Color::from_hex(0x1F242C),
                input_bg_focus: Color::from_hex(0x181C22),
                input_bg_disabled: Color::from_hex(0x13161B),
                selection: Color::from_hex(0x4F94FF).with_alpha(0.34),
                selection_text: Color::from_hex(0xF2F4F7),
                accent: Color::from_hex(0x4F94FF),
                accent_subtle: Color::from_hex(0x4F94FF).with_alpha(0.14),
                tooltip_bg: Color::from_hex(0xF2F4F7),
                tooltip_text: Color::from_hex(0x14181F),
            },
            typography: universal_typography(15.0),
            spacing: SpacingTokens::default(),
            radii: RadiusTokens {
                radius_none: 0.0,
                radius_sm: 3.0,
                // Sits at `smoothing_threshold` (12.0) so default
                // corners engage the Restrained squircle. Below the
                // threshold every corner falls back to a true circle
                // (see `resolve_corner_shape`) and the variant's
                // signature shape never reaches the most common
                // surfaces (buttons, inputs). `radius_md` (8.0)
                // stays smaller — default > md is intentional for
                // this token: `default` is "what unmarked surfaces
                // get", not a strict ladder step.
                radius_default: 12.0,
                radius_md: 8.0,
                radius_lg: 10.0,
                radius_xl: 14.0,
                radius_2xl: 18.0,
                radius_3xl: 24.0,
                radius_full: 9999.0,
            },
            shape: ShapeTokens {
                corner_smoothing: 0.65,
                corner_exponent: 4.4,
                smoothing_threshold: 12.0,
            },
            shadows: restrained_shadows_dark(),
            animations: AnimationTokens {
                duration_fastest: 75,
                duration_faster: 100,
                duration_fast: 150,
                duration_normal: 200,
                duration_slow: 280,
                duration_slower: 360,
                duration_slowest: 460,
                ease_default: STANDARD,
                ease_in: Easing::EaseIn,
                ease_out: STANDARD,
                ease_in_out: Easing::EaseInOut,
                // Apple HIG-leaning semantic slots: snappy state
                // feedback, long-tail navigation, light spring.
                ease_state: STANDARD,
                ease_nav: HIG_SHEET,
                ease_spring: SUBTLE_SPRING,
                ease_sheet: HIG_SHEET,
            },
        }
    }

    /// Bundle the light + dark variants.
    pub fn bundle() -> ThemeBundle {
        ThemeBundle::new("Universal · Restrained", Self::light(), Self::dark())
    }
}

impl Theme for RestrainedTheme {
    fn name(&self) -> &str {
        "Universal · Restrained"
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

// HIG-style quiet ease-out.
const STANDARD: Easing = Easing::CubicBezier(0.25, 0.10, 0.25, 1.0);

// iOS-style sheet / nav decelerate — long tail so surfaces glide.
const HIG_SHEET: Easing = Easing::CubicBezier(0.32, 0.72, 0.0, 1.0);

// Subtle overshoot for popovers / badges (~3% past target).
const SUBTLE_SPRING: Easing = Easing::CubicBezier(0.34, 1.20, 0.64, 1.0);

// Restrained stacks a tight ambient (1 px ink @ low alpha) + a wider
// diffuse key per elevation — Apple's HIG shadow recipe distilled.
// The ambient layer anchors the element to its surface; the key layer
// gives the lift.
fn restrained_shadows_light() -> ShadowTokens {
    let ink = Color::from_hex(0x0F141E);
    ShadowTokens {
        shadow_sm: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.04)),
            Shadow::new(0.0, 1.0, 1.5, 0.0, ink.with_alpha(0.06)),
        ],
        shadow_default: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.05)),
            Shadow::new(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.07)),
        ],
        shadow_md: vec![
            Shadow::new(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.04)),
            Shadow::new(0.0, 4.0, 8.0, 0.0, ink.with_alpha(0.05)),
        ],
        shadow_lg: vec![
            Shadow::new(0.0, 2.0, 4.0, 0.0, ink.with_alpha(0.05)),
            Shadow::new(0.0, 12.0, 20.0, 0.0, ink.with_alpha(0.07)),
        ],
        shadow_xl: vec![
            Shadow::new(0.0, 4.0, 8.0, 0.0, ink.with_alpha(0.06)),
            Shadow::new(0.0, 22.0, 36.0, 0.0, ink.with_alpha(0.09)),
        ],
        shadow_2xl: vec![
            Shadow::new(0.0, 6.0, 12.0, 0.0, ink.with_alpha(0.08)),
            Shadow::new(0.0, 32.0, 64.0, 0.0, ink.with_alpha(0.14)),
        ],
        shadow_inner: vec![Shadow::new_inset(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.06))],
        shadow_none: Vec::new(),
    }
}

fn restrained_shadows_dark() -> ShadowTokens {
    let ink = Color::BLACK;
    ShadowTokens {
        shadow_sm: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.20)),
            Shadow::new(0.0, 1.0, 1.5, 0.0, ink.with_alpha(0.30)),
        ],
        shadow_default: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.25)),
            Shadow::new(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.40)),
        ],
        shadow_md: vec![
            Shadow::new(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.22)),
            Shadow::new(0.0, 4.0, 8.0, 0.0, ink.with_alpha(0.32)),
        ],
        shadow_lg: vec![
            Shadow::new(0.0, 2.0, 4.0, 0.0, ink.with_alpha(0.26)),
            Shadow::new(0.0, 12.0, 20.0, 0.0, ink.with_alpha(0.38)),
        ],
        shadow_xl: vec![
            Shadow::new(0.0, 4.0, 8.0, 0.0, ink.with_alpha(0.30)),
            Shadow::new(0.0, 22.0, 36.0, 0.0, ink.with_alpha(0.44)),
        ],
        shadow_2xl: vec![
            Shadow::new(0.0, 6.0, 12.0, 0.0, ink.with_alpha(0.36)),
            Shadow::new(0.0, 32.0, 64.0, 0.0, ink.with_alpha(0.58)),
        ],
        shadow_inner: vec![Shadow::new_inset(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.30))],
        shadow_none: Vec::new(),
    }
}
