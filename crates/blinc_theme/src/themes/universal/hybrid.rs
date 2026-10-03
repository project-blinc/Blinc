//! Universal HID — **Hybrid** variant (the recommended default).
//!
//! Synthesises Apple HIG and Material 3 principles into a true
//! 50/50 design: tight radii on inputs / chips, bold radii on
//! cards / sheets; dual-layer shadows (ambient + key); adaptive
//! motion that picks quick ease-out for state changes and
//! emphasised easing for navigation. Mid-strength squircle
//! (smoothing 0.40, exponent 3.3) — perceptual sweet spot.
//!
//! Use as the no-match fallback in
//! [`platform_theme_bundle`](crate::platform_theme_bundle), and as
//! a friendly default for any cross-platform app.

use crate::theme::{ColorScheme, Theme, ThemeBundle};
use crate::tokens::*;
use blinc_core::Color;

/// Universal HID · Hybrid theme.
#[derive(Clone, Debug)]
pub struct HybridTheme {
    scheme: ColorScheme,
    colors: ColorTokens,
    typography: TypographyTokens,
    spacing: SpacingTokens,
    radii: RadiusTokens,
    shape: ShapeTokens,
    shadows: ShadowTokens,
    animations: AnimationTokens,
}

impl HybridTheme {
    /// Hybrid · Light.
    pub fn light() -> Self {
        Self {
            scheme: ColorScheme::Light,
            colors: ColorTokens {
                primary: Color::from_hex(0x2A63E9),
                primary_hover: Color::from_hex(0x1F52D1),
                primary_active: Color::from_hex(0x1846B5),
                secondary: Color::from_hex(0x605C73),
                secondary_hover: Color::from_hex(0x4B475C),
                secondary_active: Color::from_hex(0x3A3748),
                // Light-scheme semantic colours — bumped to HIG-vibrant
                // values (Tailwind-600 range) so status pips / chart
                // marks / toast accents read as crisp on white surfaces.
                // Still clears AA-large / UI-accent contrast against
                // Surface and Background.
                success: Color::from_hex(0x16A34A),
                success_bg: Color::from_hex(0x16A34A).with_alpha(0.12),
                warning: Color::from_hex(0xD97706),
                warning_bg: Color::from_hex(0xD97706).with_alpha(0.12),
                error: Color::from_hex(0xDC2626),
                error_bg: Color::from_hex(0xDC2626).with_alpha(0.12),
                info: Color::from_hex(0x1283C7),
                info_bg: Color::from_hex(0x1283C7).with_alpha(0.12),
                background: Color::from_hex(0xF6F8FC),
                surface: Color::WHITE,
                surface_elevated: Color::from_hex(0xFBFCFE),
                surface_overlay: Color::from_hex(0xE7ECF6),
                text_primary: Color::from_hex(0x0F1422),
                text_secondary: Color::from_hex(0x525A6E),
                text_tertiary: Color::from_hex(0x8F95A6),
                text_inverse: Color::WHITE,
                text_link: Color::from_hex(0x2A63E9),
                border: Color::from_hex(0x0F1422).with_alpha(0.10),
                border_secondary: Color::from_hex(0xD4D9E4),
                border_hover: Color::from_hex(0x0F1422).with_alpha(0.16),
                border_focus: Color::from_hex(0x2A63E9),
                border_error: Color::from_hex(0xC7382D),
                input_bg: Color::WHITE,
                input_bg_hover: Color::from_hex(0xF8FAFD),
                input_bg_focus: Color::WHITE,
                input_bg_disabled: Color::from_hex(0xEEF0F6),
                selection: Color::from_hex(0x2A63E9).with_alpha(0.24),
                selection_text: Color::from_hex(0x0F1422),
                accent: Color::from_hex(0x2A63E9),
                accent_subtle: Color::from_hex(0xE6EEFE),
                tooltip_bg: Color::from_hex(0x161A28),
                tooltip_text: Color::from_hex(0xF6F8FC),
            },
            typography: universal_typography(15.0),
            spacing: SpacingTokens::default(),
            radii: RadiusTokens {
                // Mixed scale: tight for inputs (sm/default/md), bold for surfaces (lg+).
                radius_none: 0.0,
                radius_sm: 4.0,
                // Sits at `smoothing_threshold` (12.0) so default
                // corners engage the Hybrid squircle. Below the
                // threshold every corner falls back to a true
                // circle, which left buttons / inputs (the most
                // common surfaces consuming `--radius-default`) on
                // the circular fallback. `radius_md` (10.0) stays
                // smaller — default > md is intentional for this
                // token: `default` is "what unmarked surfaces get",
                // not a strict ladder step.
                radius_default: 12.0,
                radius_md: 10.0,
                radius_lg: 14.0,
                radius_xl: 18.0,
                radius_2xl: 24.0,
                radius_3xl: 32.0,
                radius_full: 9999.0,
            },
            shape: ShapeTokens {
                corner_smoothing: 0.40,
                corner_exponent: 3.3,
                smoothing_threshold: 12.0,
            },
            // Outermost layer of each dual-layer compound shadow from the
            // canvas — captures ~80% of the depth signal. See the
            // out-of-scope note in the plan re Vec<Shadow> follow-up.
            shadows: hybrid_shadows_light(),
            animations: AnimationTokens {
                // Adaptive: state changes use quick ease-out;
                // navigation/sheets use emphasised easing.
                duration_fastest: 80,
                duration_faster: 120,
                duration_fast: 180,
                duration_normal: 240,
                duration_slow: 320,
                duration_slower: 420,
                duration_slowest: 540,
                ease_default: STANDARD,
                ease_in: Easing::EaseIn,
                ease_out: Easing::EaseOut,
                ease_in_out: Easing::EaseInOut,
                // Hybrid is the cross-platform default — blends HIG
                // snappiness with a touch of Material spring on the
                // attention-grabbing surfaces.
                ease_state: STANDARD,
                ease_nav: HYBRID_NAV,
                ease_spring: HYBRID_SPRING,
                ease_sheet: HYBRID_NAV,
            },
        }
    }

    /// Hybrid · Dark.
    pub fn dark() -> Self {
        Self {
            scheme: ColorScheme::Dark,
            colors: ColorTokens {
                primary: Color::from_hex(0x7DA8FF),
                primary_hover: Color::from_hex(0x94B8FF),
                primary_active: Color::from_hex(0xACC8FF),
                secondary: Color::from_hex(0xA8A4B8),
                secondary_hover: Color::from_hex(0xBFBBCC),
                secondary_active: Color::from_hex(0xD6D3E0),
                success: Color::from_hex(0x54D281),
                success_bg: Color::from_hex(0x54D281).with_alpha(0.15),
                warning: Color::from_hex(0xF0B14B),
                warning_bg: Color::from_hex(0xF0B14B).with_alpha(0.15),
                error: Color::from_hex(0xFF6F5F),
                error_bg: Color::from_hex(0xFF6F5F).with_alpha(0.15),
                info: Color::from_hex(0x6BC0EE),
                info_bg: Color::from_hex(0x6BC0EE).with_alpha(0.15),
                background: Color::from_hex(0x0F1320),
                surface: Color::from_hex(0x1A1F2E),
                surface_elevated: Color::from_hex(0x232940),
                surface_overlay: Color::from_hex(0x0A0D17),
                text_primary: Color::from_hex(0xECEEF4),
                text_secondary: Color::from_hex(0xADB3C2),
                text_tertiary: Color::from_hex(0x737A8C),
                text_inverse: Color::from_hex(0x0F1320),
                text_link: Color::from_hex(0x7DA8FF),
                border: Color::WHITE.with_alpha(0.10),
                border_secondary: Color::from_hex(0x3A4055),
                border_hover: Color::WHITE.with_alpha(0.18),
                border_focus: Color::from_hex(0x7DA8FF),
                border_error: Color::from_hex(0xFF6F5F),
                input_bg: Color::from_hex(0x1A1F2E),
                input_bg_hover: Color::from_hex(0x222740),
                input_bg_focus: Color::from_hex(0x1A1F2E),
                input_bg_disabled: Color::from_hex(0x13172A),
                selection: Color::from_hex(0x7DA8FF).with_alpha(0.32),
                selection_text: Color::from_hex(0xECEEF4),
                accent: Color::from_hex(0x7DA8FF),
                accent_subtle: Color::from_hex(0x7DA8FF).with_alpha(0.16),
                tooltip_bg: Color::from_hex(0xECEEF4),
                tooltip_text: Color::from_hex(0x0F1320),
            },
            typography: universal_typography(15.0),
            spacing: SpacingTokens::default(),
            radii: RadiusTokens {
                radius_none: 0.0,
                radius_sm: 4.0,
                // Sits at `smoothing_threshold` (12.0) so default
                // corners engage the Hybrid squircle. Below the
                // threshold every corner falls back to a true
                // circle, which left buttons / inputs (the most
                // common surfaces consuming `--radius-default`) on
                // the circular fallback. `radius_md` (10.0) stays
                // smaller — default > md is intentional for this
                // token: `default` is "what unmarked surfaces get",
                // not a strict ladder step.
                radius_default: 12.0,
                radius_md: 10.0,
                radius_lg: 14.0,
                radius_xl: 18.0,
                radius_2xl: 24.0,
                radius_3xl: 32.0,
                radius_full: 9999.0,
            },
            shape: ShapeTokens {
                corner_smoothing: 0.40,
                corner_exponent: 3.3,
                smoothing_threshold: 12.0,
            },
            shadows: hybrid_shadows_dark(),
            animations: AnimationTokens {
                duration_fastest: 80,
                duration_faster: 120,
                duration_fast: 180,
                duration_normal: 240,
                duration_slow: 320,
                duration_slower: 420,
                duration_slowest: 540,
                ease_default: STANDARD,
                ease_in: Easing::EaseIn,
                ease_out: Easing::EaseOut,
                ease_in_out: Easing::EaseInOut,
                // Hybrid is the cross-platform default — blends HIG
                // snappiness with a touch of Material spring on the
                // attention-grabbing surfaces.
                ease_state: STANDARD,
                ease_nav: HYBRID_NAV,
                ease_spring: HYBRID_SPRING,
                ease_sheet: HYBRID_NAV,
            },
        }
    }

    /// Bundle the light + dark variants.
    pub fn bundle() -> ThemeBundle {
        ThemeBundle::new("Universal · Hybrid", Self::light(), Self::dark())
    }
}

impl Theme for HybridTheme {
    fn name(&self) -> &str {
        "Universal · Hybrid"
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

// Apple-leaning quiet ease-out — the "standard" easing in HIG.
// Used as `ease_default` for Restrained + Hybrid (Expressive overrides
// to emphasised-decel).
const STANDARD: Easing = Easing::CubicBezier(0.25, 0.10, 0.25, 1.0);

// Hybrid navigation decelerate — between HIG_SHEET (Restrained) and
// EMPHASIZED (Expressive). Reads smooth without overshoot.
const HYBRID_NAV: Easing = Easing::CubicBezier(0.30, 0.70, 0.10, 1.0);

// Mid-strength spring for popovers / badges (~6% past target).
const HYBRID_SPRING: Easing = Easing::CubicBezier(0.34, 1.30, 0.64, 1.0);

/// Shared typography for all three Universal variants.
///
/// Noto Sans is Blinc's universal fallback on platforms without a
/// system font; promoting it to the canonical sans means the
/// Universal theme renders identically regardless of platform font
/// resolution.
///
/// The size ladder is the Universal HID-tuned scale (12 / 13 / 15 /
/// 17 / …) — one step tighter than the default Tailwind-inspired
/// scale (12 / 14 / 16 / 18 / …) per the design doc's "HID density"
/// recommendation. `text_base` parameter lets a variant override
/// the body size if needed (all three currently pass 15).
pub(super) fn universal_typography(text_base: f32) -> TypographyTokens {
    TypographyTokens {
        font_sans: FontFamily::new(
            "Noto Sans",
            vec![
                "system-ui",
                "-apple-system",
                "Segoe UI",
                "Roboto",
                "sans-serif",
            ],
        ),
        font_serif: FontFamily::new("Noto Serif", vec!["ui-serif", "Georgia", "serif"]),
        font_mono: FontFamily::new(
            "Noto Sans Mono",
            vec!["ui-monospace", "SF Mono", "Menlo", "monospace"],
        ),
        text_xs: 12.0,
        text_sm: 13.0, // vs default 14 — HID density
        text_base,     // = 15 (vs default 16)
        text_lg: 17.0, // vs default 18
        text_xl: 20.0,
        text_2xl: 24.0,
        text_3xl: 30.0,
        text_4xl: 36.0,
        text_5xl: 48.0,
        ..Default::default()
    }
}

// Hybrid uses dual-layer shadows: a tight 1 px inner ambient + a
// broader key-light layer per elevation. Balanced between
// Restrained's quiet single-layer feel and Expressive's bolder
// Material recipe.
fn hybrid_shadows_light() -> ShadowTokens {
    let ink = Color::from_hex(0x0F1422);
    ShadowTokens {
        shadow_sm: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.05)),
            Shadow::new(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.06)),
        ],
        shadow_default: vec![
            Shadow::new(0.0, 1.0, 1.0, 0.0, ink.with_alpha(0.05)),
            Shadow::new(0.0, 2.0, 4.0, 0.0, ink.with_alpha(0.06)),
        ],
        shadow_md: vec![
            Shadow::new(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.05)),
            Shadow::new(0.0, 4.0, 8.0, 0.0, ink.with_alpha(0.07)),
        ],
        shadow_lg: vec![
            Shadow::new(0.0, 3.0, 6.0, 0.0, ink.with_alpha(0.06)),
            Shadow::new(0.0, 12.0, 22.0, 0.0, ink.with_alpha(0.08)),
        ],
        shadow_xl: vec![
            Shadow::new(0.0, 6.0, 12.0, 0.0, ink.with_alpha(0.07)),
            Shadow::new(0.0, 24.0, 40.0, 0.0, ink.with_alpha(0.10)),
        ],
        shadow_2xl: vec![
            Shadow::new(0.0, 10.0, 20.0, 0.0, ink.with_alpha(0.09)),
            Shadow::new(0.0, 40.0, 64.0, 0.0, ink.with_alpha(0.14)),
        ],
        shadow_inner: vec![Shadow::new_inset(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.07))],
        shadow_none: Vec::new(),
    }
}

fn hybrid_shadows_dark() -> ShadowTokens {
    let ink = Color::BLACK;
    ShadowTokens {
        shadow_sm: vec![
            Shadow::new(0.0, 0.0, 1.0, 0.0, ink.with_alpha(0.22)),
            Shadow::new(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.35)),
        ],
        shadow_default: vec![
            Shadow::new(0.0, 1.0, 1.0, 0.0, ink.with_alpha(0.26)),
            Shadow::new(0.0, 2.0, 4.0, 0.0, ink.with_alpha(0.40)),
        ],
        shadow_md: vec![
            Shadow::new(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.28)),
            Shadow::new(0.0, 4.0, 8.0, 0.0, ink.with_alpha(0.42)),
        ],
        shadow_lg: vec![
            Shadow::new(0.0, 3.0, 6.0, 0.0, ink.with_alpha(0.30)),
            Shadow::new(0.0, 12.0, 22.0, 0.0, ink.with_alpha(0.44)),
        ],
        shadow_xl: vec![
            Shadow::new(0.0, 6.0, 12.0, 0.0, ink.with_alpha(0.34)),
            Shadow::new(0.0, 24.0, 40.0, 0.0, ink.with_alpha(0.50)),
        ],
        shadow_2xl: vec![
            Shadow::new(0.0, 10.0, 20.0, 0.0, ink.with_alpha(0.40)),
            Shadow::new(0.0, 40.0, 64.0, 0.0, ink.with_alpha(0.58)),
        ],
        shadow_inner: vec![Shadow::new_inset(0.0, 1.0, 2.0, 0.0, ink.with_alpha(0.32))],
        shadow_none: Vec::new(),
    }
}
