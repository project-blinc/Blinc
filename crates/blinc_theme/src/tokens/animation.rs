//! Animation/transition tokens for theming

/// Semantic animation token keys for dynamic access
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum AnimationToken {
    // Durations
    DurationFastest,
    DurationFaster,
    DurationFast,
    DurationNormal,
    DurationSlow,
    DurationSlower,
    DurationSlowest,
}

/// Easing function type
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Easing {
    Linear,
    EaseIn,
    #[default]
    EaseOut,
    EaseInOut,
    /// Custom cubic bezier (x1, y1, x2, y2)
    CubicBezier(f32, f32, f32, f32),
}

impl Easing {
    /// Convert to the richer `blinc_animation::Easing` used by
    /// keyframe presets / `MultiKeyframeAnimation`. The theme variant
    /// is intentionally simpler (the design tokens only need the four
    /// CSS-named curves + custom cubic-bezier), but every theme
    /// curve maps cleanly onto an animation easing.
    pub fn to_animation_easing(self) -> blinc_animation::Easing {
        match self {
            Easing::Linear => blinc_animation::Easing::Linear,
            Easing::EaseIn => blinc_animation::Easing::EaseIn,
            Easing::EaseOut => blinc_animation::Easing::EaseOut,
            Easing::EaseInOut => blinc_animation::Easing::EaseInOut,
            Easing::CubicBezier(a, b, c, d) => blinc_animation::Easing::CubicBezier(a, b, c, d),
        }
    }

    /// The cubic-bezier control points this easing denotes, or `None` for
    /// `Linear`.
    ///
    /// One source of truth, so the curve a theme evaluates and the
    /// `cubic-bezier()` it exports to CSS cannot drift apart. Every named
    /// variant used to evaluate a hand-written polynomial while exporting
    /// different control points, and `CubicBezier` ignored its own x pair
    /// entirely.
    pub fn control_points(&self) -> Option<[f32; 4]> {
        match *self {
            Easing::Linear => None,
            Easing::EaseIn => Some([0.4, 0.0, 1.0, 1.0]),
            Easing::EaseOut => Some([0.0, 0.0, 0.2, 1.0]),
            Easing::EaseInOut => Some([0.4, 0.0, 0.2, 1.0]),
            Easing::CubicBezier(x1, y1, x2, y2) => Some([x1, y1, x2, y2]),
        }
    }

    /// Evaluate the easing function at time t (0.0 to 1.0).
    ///
    /// A cubic-bezier easing is y at the parameter s where x(s) = t, not y
    /// at t. Solving for s is what `blinc_animation` already does, so this
    /// defers to it rather than keeping a second implementation.
    ///
    /// An overshoot curve such as a spring's returns values above 1, which
    /// is the point of it; only the input is clamped.
    pub fn evaluate(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self.control_points() {
            None => t,
            Some([x1, y1, x2, y2]) => blinc_animation::Easing::CubicBezier(x1, y1, x2, y2).apply(t),
        }
    }
}

/// Complete set of animation tokens
#[derive(Clone, Debug)]
pub struct AnimationTokens {
    // Durations in milliseconds
    pub duration_fastest: u64,
    pub duration_faster: u64,
    pub duration_fast: u64,
    pub duration_normal: u64,
    pub duration_slow: u64,
    pub duration_slower: u64,
    pub duration_slowest: u64,

    // Generic easing functions (curve-shape semantics).
    pub ease_default: Easing,
    pub ease_in: Easing,
    pub ease_out: Easing,
    pub ease_in_out: Easing,

    // Semantic easing roles (intent-shape semantics). Each maps onto
    // a class of UI motion so callers can ask for the curve that
    // matches the *meaning* of the motion rather than picking
    // `ease_out` everywhere. Universal HID variants override these
    // with platform-appropriate curves; the default impl falls back
    // to the generic slots so older themes that haven't been
    // migrated stay source-compatible.
    /// State-change feedback (hover, press, focus, checked).
    /// Snappy, short — should resolve within `duration_fast`.
    pub ease_state: Easing,
    /// Navigation transitions (page change, tab switch, route).
    /// Smooth, directional — typically a long-tail decelerate.
    pub ease_nav: Easing,
    /// Spring-like motion (popovers, badges, attention nudges).
    /// Overshoots slightly to draw the eye.
    pub ease_spring: Easing,
    /// Modal sheet / drawer slide-in. Heavier curve than `state` so
    /// the surface reads as substantial — long-tail decelerate.
    pub ease_sheet: Easing,
}

impl AnimationTokens {
    /// Get duration by token key (in milliseconds)
    pub fn get(&self, token: AnimationToken) -> u64 {
        match token {
            AnimationToken::DurationFastest => self.duration_fastest,
            AnimationToken::DurationFaster => self.duration_faster,
            AnimationToken::DurationFast => self.duration_fast,
            AnimationToken::DurationNormal => self.duration_normal,
            AnimationToken::DurationSlow => self.duration_slow,
            AnimationToken::DurationSlower => self.duration_slower,
            AnimationToken::DurationSlowest => self.duration_slowest,
        }
    }

    /// Get duration as seconds (f32)
    pub fn get_seconds(&self, token: AnimationToken) -> f32 {
        self.get(token) as f32 / 1000.0
    }
}

impl Default for AnimationTokens {
    fn default() -> Self {
        Self {
            duration_fastest: 75,
            duration_faster: 100,
            duration_fast: 150,
            duration_normal: 200,
            duration_slow: 300,
            duration_slower: 400,
            duration_slowest: 500,

            ease_default: Easing::EaseOut,
            ease_in: Easing::EaseIn,
            ease_out: Easing::EaseOut,
            ease_in_out: Easing::EaseInOut,

            // Generic fallbacks. Universal HID variants override
            // these with platform-appropriate curves.
            ease_state: Easing::EaseOut,
            ease_nav: Easing::EaseInOut,
            ease_spring: Easing::EaseOut,
            ease_sheet: Easing::EaseOut,
        }
    }
}

#[cfg(test)]
mod easing_tests {
    use super::*;

    /// The theme's STANDARD curve is CSS `ease`. A browser gives 0.8024 at
    /// the halfway point; evaluating the y polynomial at t instead gives
    /// 0.5375, which is what this used to return.
    #[test]
    fn standard_curve_matches_css_ease() {
        let standard = Easing::CubicBezier(0.25, 0.10, 0.25, 1.0);
        let got = standard.evaluate(0.5);
        assert!(
            (got - 0.8024).abs() < 0.001,
            "want CSS's 0.8024 at t=0.5, got {got:.4} \
             (0.5375 means the x control points are being ignored)"
        );
    }

    /// The x pair has to change the answer. Two curves with identical y
    /// control points and different x ones must not agree.
    #[test]
    fn the_x_control_points_matter() {
        let a = Easing::CubicBezier(0.25, 0.10, 0.25, 1.0).evaluate(0.5);
        let b = Easing::CubicBezier(0.90, 0.10, 0.90, 1.0).evaluate(0.5);
        assert!(
            (a - b).abs() > 0.1,
            "x control points are being discarded: both curves gave {a:.4}"
        );
    }

    /// A spring curve's whole purpose is to pass its target and come back,
    /// so evaluate must be free to exceed 1 even though t is clamped.
    #[test]
    fn a_spring_curve_overshoots() {
        let spring = Easing::CubicBezier(0.34, 1.30, 0.64, 1.0);
        let peak = (0..=100)
            .map(|i| spring.evaluate(i as f32 / 100.0))
            .fold(0.0f32, f32::max);
        assert!(peak > 1.0, "spring never passed 1, peak was {peak:.4}");
    }

    /// Endpoints are exact for every curve, or a transition starts or ends
    /// with a visible jump.
    #[test]
    fn endpoints_are_exact() {
        for e in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::CubicBezier(0.25, 0.10, 0.25, 1.0),
            Easing::CubicBezier(0.34, 1.30, 0.64, 1.0),
        ] {
            assert!(e.evaluate(0.0).abs() < 1e-6, "{e:?} does not start at 0");
            assert!(
                (e.evaluate(1.0) - 1.0).abs() < 1e-6,
                "{e:?} does not end at 1"
            );
        }
    }

    /// Every named variant must evaluate the curve it exports to CSS.
    /// They diverged before: `EaseIn` computed `t * t` while exporting
    /// `cubic-bezier(0.4, 0, 1, 1)`.
    #[test]
    fn named_variants_evaluate_what_they_export() {
        for e in [Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut] {
            let [x1, y1, x2, y2] = e.control_points().expect("not linear");
            let direct = Easing::CubicBezier(x1, y1, x2, y2);
            for i in 0..=10 {
                let t = i as f32 / 10.0;
                assert!(
                    (e.evaluate(t) - direct.evaluate(t)).abs() < 1e-6,
                    "{e:?} at t={t} differs from its own control points"
                );
            }
        }
    }
}
