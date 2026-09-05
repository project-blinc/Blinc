//! Lowering CSS timing enums to the animation engine's types.

use blinc_theme::ThemeState;

use crate::parser::*;

#[test]
fn test_animation_timing_to_easing() {
    use blinc_animation::Easing;

    assert!(matches!(
        AnimationTiming::Linear.to_easing(),
        Easing::Linear
    ));
    // CSS ease = cubic-bezier(0.25, 0.1, 0.25, 1.0)
    assert!(matches!(
        AnimationTiming::Ease.to_easing(),
        Easing::CubicBezier(0.25, 0.1, 0.25, 1.0)
    ));
    // CSS ease-in = cubic-bezier(0.42, 0.0, 1.0, 1.0)
    assert!(matches!(
        AnimationTiming::EaseIn.to_easing(),
        Easing::CubicBezier(0.42, 0.0, 1.0, 1.0)
    ));
    // CSS ease-out = cubic-bezier(0.0, 0.0, 0.58, 1.0)
    assert!(matches!(
        AnimationTiming::EaseOut.to_easing(),
        Easing::CubicBezier(0.0, 0.0, 0.58, 1.0)
    ));
    // CSS ease-in-out = cubic-bezier(0.42, 0.0, 0.58, 1.0)
    assert!(matches!(
        AnimationTiming::EaseInOut.to_easing(),
        Easing::CubicBezier(0.42, 0.0, 0.58, 1.0)
    ));
}

#[test]
fn test_animation_direction_to_play_direction() {
    use blinc_animation::PlayDirection;

    assert!(matches!(
        AnimationDirection::Normal.to_play_direction(),
        PlayDirection::Forward
    ));
    assert!(matches!(
        AnimationDirection::Reverse.to_play_direction(),
        PlayDirection::Reverse
    ));
    assert!(matches!(
        AnimationDirection::Alternate.to_play_direction(),
        PlayDirection::Alternate
    ));
    assert!(matches!(
        AnimationDirection::AlternateReverse.to_play_direction(),
        PlayDirection::Alternate
    ));
}

#[test]
fn test_animation_direction_starts_reversed() {
    assert!(!AnimationDirection::Normal.starts_reversed());
    assert!(!AnimationDirection::Reverse.starts_reversed());
    assert!(!AnimationDirection::Alternate.starts_reversed());
    assert!(AnimationDirection::AlternateReverse.starts_reversed());
}

#[test]
fn test_animation_fill_mode_to_fill_mode() {
    use blinc_animation::FillMode;

    assert!(matches!(
        AnimationFillMode::None.to_fill_mode(),
        FillMode::None
    ));
    assert!(matches!(
        AnimationFillMode::Forwards.to_fill_mode(),
        FillMode::Forwards
    ));
    assert!(matches!(
        AnimationFillMode::Backwards.to_fill_mode(),
        FillMode::Backwards
    ));
    assert!(matches!(
        AnimationFillMode::Both.to_fill_mode(),
        FillMode::Both
    ));
}

#[test]
fn test_resolve_keyframe_animation_preserves_all_keyframes() {
    ThemeState::init_default();

    let css = r#"
        @keyframes pulse {
            0% { opacity: 1; }
            50% { opacity: 0.5; }
            100% { opacity: 1; }
        }
        #button { animation: pulse 1000ms ease-in-out infinite; }
    "#;

    let result = Stylesheet::parse_with_errors(css);
    assert!(!result.has_errors());

    // Verify keyframes are stored
    let keyframes = result.stylesheet.get_keyframes("pulse").unwrap();
    assert_eq!(keyframes.keyframes.len(), 3); // All 3 keyframes preserved

    // Resolve animation - should preserve all keyframes
    let animation = result.stylesheet.resolve_keyframe_animation("button");
    assert!(animation.is_some());

    let anim = animation.unwrap();
    // The animation should have the correct duration
    assert_eq!(anim.duration_ms(), 1000);
}

#[test]
fn test_resolve_keyframe_animation_with_state() {
    ThemeState::init_default();

    let css = r#"
        @keyframes hover-glow {
            from { opacity: 0.8; }
            to { opacity: 1.0; }
        }
        #button:hover { animation: hover-glow 200ms ease-out; }
    "#;

    let result = Stylesheet::parse_with_errors(css);
    assert!(!result.has_errors());

    // Base style should not have animation
    let base_anim = result.stylesheet.resolve_keyframe_animation("button");
    assert!(base_anim.is_none());

    // Hover state should have animation
    let hover_anim = result
        .stylesheet
        .resolve_keyframe_animation_with_state("button", ElementState::Hover);
    assert!(hover_anim.is_some());

    let anim = hover_anim.unwrap();
    assert_eq!(anim.duration_ms(), 200);
}

#[test]
fn test_css_animation_iteration_count() {
    ThemeState::init_default();

    // Test infinite animation
    let css_infinite = r#"
        @keyframes spin { from { opacity: 0; } to { opacity: 1; } }
        #a { animation: spin 1s infinite; }
    "#;
    let result = Stylesheet::parse_with_errors(css_infinite);
    let style = result.stylesheet.get("a").unwrap();
    assert_eq!(style.animation.as_ref().unwrap().iteration_count, 0); // 0 = infinite

    // Test finite animation
    let css_finite = r#"
        @keyframes spin { from { opacity: 0; } to { opacity: 1; } }
        #b { animation: spin 1s 3; }
    "#;
    let result = Stylesheet::parse_with_errors(css_finite);
    let style = result.stylesheet.get("b").unwrap();
    assert_eq!(style.animation.as_ref().unwrap().iteration_count, 3);
}

// ====================================================================
// Property Aliases
// ====================================================================
