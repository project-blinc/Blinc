//! Shadow tokens for theming
//!
//! Each elevation slot is a STACK of one or more `Shadow` layers. Single-
//! layer themes (every platform bundle + `BlincTheme`) just wrap a single
//! shadow in a one-element stack. Multi-layer Universal HID variants
//! stack 2-3 shadows per slot — usually one tight inner-ambient layer +
//! one wider key-light layer — to match the depth recipes the design
//! doc specifies.
//!
//! The renderer iterates the stack back-to-front and emits one GPU
//! shadow primitive per layer.

use blinc_core::Color;

/// Semantic shadow token keys for dynamic access
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum ShadowToken {
    Sm,
    Default,
    Md,
    Lg,
    Xl,
    Xxl,
    Inner,
    None,
}

/// A box shadow definition. Multiple `Shadow`s stacked into a
/// `Vec<Shadow>` form a compound shadow (see module docs).
#[derive(Clone, Debug)]
pub struct Shadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
    /// Painted inside the shape, as CSS `box-shadow: inset`.
    pub inset: bool,
}

impl Shadow {
    pub const fn new(offset_x: f32, offset_y: f32, blur: f32, spread: f32, color: Color) -> Self {
        Self {
            offset_x,
            offset_y,
            blur,
            spread,
            color,
            inset: false,
        }
    }

    /// An inset shadow. Separate from `new` so that stays a 5-arg
    /// `const fn` and no existing token changes.
    pub const fn new_inset(
        offset_x: f32,
        offset_y: f32,
        blur: f32,
        spread: f32,
        color: Color,
    ) -> Self {
        Self {
            inset: true,
            ..Self::new(offset_x, offset_y, blur, spread, color)
        }
    }

    pub const fn none() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
            color: Color::TRANSPARENT,
            inset: false,
        }
    }

    /// Linear interpolation between two shadows
    pub fn lerp(from: &Self, to: &Self, t: f32) -> Self {
        Self {
            offset_x: from.offset_x + (to.offset_x - from.offset_x) * t,
            offset_y: from.offset_y + (to.offset_y - from.offset_y) * t,
            blur: from.blur + (to.blur - from.blur) * t,
            spread: from.spread + (to.spread - from.spread) * t,
            color: Color::lerp(&from.color, &to.color, t),
            // Not interpolable: a shadow is inset or it is not. Hold the
            // start's value; the transition assigns the target on its
            // final frame.
            inset: from.inset,
        }
    }
}

impl Default for Shadow {
    fn default() -> Self {
        Self::none()
    }
}

impl From<Shadow> for blinc_core::Shadow {
    fn from(shadow: Shadow) -> Self {
        blinc_core::Shadow {
            offset_x: shadow.offset_x,
            offset_y: shadow.offset_y,
            blur: shadow.blur,
            spread: shadow.spread,
            color: shadow.color,
            inset: shadow.inset,
        }
    }
}

impl From<&Shadow> for blinc_core::Shadow {
    fn from(shadow: &Shadow) -> Self {
        blinc_core::Shadow {
            offset_x: shadow.offset_x,
            offset_y: shadow.offset_y,
            blur: shadow.blur,
            spread: shadow.spread,
            color: shadow.color,
            inset: shadow.inset,
        }
    }
}

/// Element-wise linear interpolation between two shadow stacks.
///
/// When stacks differ in length, the shorter side is padded with
/// transparent `Shadow::none()` so the extra layers fade in/out
/// during the lerp instead of popping. This matches the way CSS
/// `box-shadow:` transitions handle mismatched layer counts.
pub fn lerp_shadow_stack(from: &[Shadow], to: &[Shadow], t: f32) -> Vec<Shadow> {
    let n = from.len().max(to.len());
    (0..n)
        .map(|i| {
            let f = from.get(i).cloned().unwrap_or_else(Shadow::none);
            let g = to.get(i).cloned().unwrap_or_else(Shadow::none);
            Shadow::lerp(&f, &g, t)
        })
        .collect()
}

/// Complete set of shadow tokens. Each field is a STACK of shadow
/// layers — single-layer themes hold a one-element `Vec<Shadow>`,
/// multi-layer Universal HID variants stack 2-3 layers per slot.
#[derive(Clone, Debug)]
pub struct ShadowTokens {
    pub shadow_sm: Vec<Shadow>,
    pub shadow_default: Vec<Shadow>,
    pub shadow_md: Vec<Shadow>,
    pub shadow_lg: Vec<Shadow>,
    pub shadow_xl: Vec<Shadow>,
    pub shadow_2xl: Vec<Shadow>,
    pub shadow_inner: Vec<Shadow>,
    pub shadow_none: Vec<Shadow>,
}

impl ShadowTokens {
    /// Get shadow stack by token key.
    pub fn get(&self, token: ShadowToken) -> &[Shadow] {
        match token {
            ShadowToken::Sm => &self.shadow_sm,
            ShadowToken::Default => &self.shadow_default,
            ShadowToken::Md => &self.shadow_md,
            ShadowToken::Lg => &self.shadow_lg,
            ShadowToken::Xl => &self.shadow_xl,
            ShadowToken::Xxl => &self.shadow_2xl,
            ShadowToken::Inner => &self.shadow_inner,
            ShadowToken::None => &self.shadow_none,
        }
    }

    /// Helper used by single-layer themes (and tests) to wrap one
    /// shadow into a one-element stack.
    pub fn single(shadow: Shadow) -> Vec<Shadow> {
        vec![shadow]
    }

    /// Create shadow tokens for a light color scheme
    pub fn light() -> Self {
        let base_color = Color::BLACK;
        Self {
            shadow_sm: vec![Shadow::new(0.0, 1.0, 2.0, 0.0, base_color.with_alpha(0.05))],
            shadow_default: vec![Shadow::new(0.0, 1.0, 3.0, 0.0, base_color.with_alpha(0.1))],
            shadow_md: vec![Shadow::new(0.0, 4.0, 6.0, -1.0, base_color.with_alpha(0.1))],
            shadow_lg: vec![Shadow::new(
                0.0,
                10.0,
                15.0,
                -3.0,
                base_color.with_alpha(0.1),
            )],
            shadow_xl: vec![Shadow::new(
                0.0,
                20.0,
                25.0,
                -5.0,
                base_color.with_alpha(0.1),
            )],
            shadow_2xl: vec![Shadow::new(
                0.0,
                25.0,
                50.0,
                -12.0,
                base_color.with_alpha(0.25),
            )],
            shadow_inner: vec![Shadow::new(0.0, 2.0, 4.0, 0.0, base_color.with_alpha(0.05))],
            shadow_none: Vec::new(),
        }
    }

    /// Create shadow tokens for a dark color scheme
    pub fn dark() -> Self {
        let base_color = Color::BLACK;
        Self {
            shadow_sm: vec![Shadow::new(0.0, 1.0, 2.0, 0.0, base_color.with_alpha(0.2))],
            shadow_default: vec![Shadow::new(0.0, 1.0, 3.0, 0.0, base_color.with_alpha(0.3))],
            shadow_md: vec![Shadow::new(0.0, 4.0, 6.0, -1.0, base_color.with_alpha(0.3))],
            shadow_lg: vec![Shadow::new(
                0.0,
                10.0,
                15.0,
                -3.0,
                base_color.with_alpha(0.3),
            )],
            shadow_xl: vec![Shadow::new(
                0.0,
                20.0,
                25.0,
                -5.0,
                base_color.with_alpha(0.3),
            )],
            shadow_2xl: vec![Shadow::new(
                0.0,
                25.0,
                50.0,
                -12.0,
                base_color.with_alpha(0.5),
            )],
            shadow_inner: vec![Shadow::new(0.0, 2.0, 4.0, 0.0, base_color.with_alpha(0.15))],
            shadow_none: Vec::new(),
        }
    }

    /// Linear interpolation between two shadow token sets. Each
    /// slot's stack is lerp'd element-wise via
    /// [`lerp_shadow_stack`] — mismatched layer counts pad with
    /// transparent layers on the shorter side so fade-in / fade-out
    /// behaves smoothly during scheme transitions.
    pub fn lerp(from: &Self, to: &Self, t: f32) -> Self {
        Self {
            shadow_sm: lerp_shadow_stack(&from.shadow_sm, &to.shadow_sm, t),
            shadow_default: lerp_shadow_stack(&from.shadow_default, &to.shadow_default, t),
            shadow_md: lerp_shadow_stack(&from.shadow_md, &to.shadow_md, t),
            shadow_lg: lerp_shadow_stack(&from.shadow_lg, &to.shadow_lg, t),
            shadow_xl: lerp_shadow_stack(&from.shadow_xl, &to.shadow_xl, t),
            shadow_2xl: lerp_shadow_stack(&from.shadow_2xl, &to.shadow_2xl, t),
            shadow_inner: lerp_shadow_stack(&from.shadow_inner, &to.shadow_inner, t),
            shadow_none: Vec::new(),
        }
    }
}

impl Default for ShadowTokens {
    fn default() -> Self {
        Self::light()
    }
}

#[cfg(test)]
mod inset_tests {
    use super::*;

    /// `shadow_inner` must be inset, or it paints as a drop shadow under
    /// the element instead of an inset one. Every theme defined the token
    /// and none of them could express it.
    #[test]
    fn the_inner_token_is_inset_in_every_theme() {
        use crate::Theme;
        use crate::themes::universal::{ExpressiveTheme, HybridTheme, RestrainedTheme};

        fn check(label: &str, theme: &dyn Theme) {
            let inner = &theme.shadows().shadow_inner;
            assert!(!inner.is_empty(), "{label}: no inner shadow");
            for s in inner {
                assert!(
                    s.inset,
                    "{label}: shadow_inner is not inset, so it draws outside"
                );
            }
        }

        check("hybrid light", &HybridTheme::light());
        check("hybrid dark", &HybridTheme::dark());
        check("restrained light", &RestrainedTheme::light());
        check("restrained dark", &RestrainedTheme::dark());
        check("expressive light", &ExpressiveTheme::light());
        check("expressive dark", &ExpressiveTheme::dark());
    }

    /// The ordinary tokens must NOT be inset.
    #[test]
    fn the_outer_tokens_are_not_inset() {
        use crate::Theme;
        let t = crate::themes::universal::HybridTheme::light();
        let sh = t.shadows();
        for (name, stack) in [
            ("sm", &sh.shadow_sm),
            ("md", &sh.shadow_md),
            ("lg", &sh.shadow_lg),
        ] {
            for s in stack {
                assert!(!s.inset, "shadow_{name} should be an outer shadow");
            }
        }
    }

    /// Crossing the FFI to the render type must keep the flag, or the
    /// paint walk never sees it.
    #[test]
    fn the_conversion_keeps_inset() {
        let inner = Shadow::new_inset(0.0, 1.0, 2.0, 0.0, Color::BLACK);
        let core: blinc_core::Shadow = (&inner).into();
        assert!(
            core.inset,
            "inset was dropped converting to the render type"
        );

        let outer = Shadow::new(0.0, 1.0, 2.0, 0.0, Color::BLACK);
        let core: blinc_core::Shadow = (&outer).into();
        assert!(!core.inset);
    }
}
