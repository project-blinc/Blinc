//! `animation` and `transition` shorthands and longhands.

use crate::parser::*;

#[test]
fn test_animation_shorthand_basic() {
    let css = r#"
        #card {
            animation: fade-in 300ms;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    assert!(!result.has_errors());

    let style = result.stylesheet.get("card").unwrap();
    let anim = style.animation.as_ref().unwrap();
    assert_eq!(anim.name, "fade-in");
    assert_eq!(anim.duration_ms, 300);
}

#[test]
fn test_animation_shorthand_full() {
    let css = r#"
        #modal {
            animation: slide-in 0.5s ease-out 100ms infinite alternate forwards;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    assert!(!result.has_errors());

    let style = result.stylesheet.get("modal").unwrap();
    let anim = style.animation.as_ref().unwrap();
    assert_eq!(anim.name, "slide-in");
    assert_eq!(anim.duration_ms, 500);
    assert_eq!(anim.timing, AnimationTiming::EaseOut);
    assert_eq!(anim.delay_ms, 100);
    assert_eq!(anim.iteration_count, 0); // 0 = infinite
    assert_eq!(anim.direction, AnimationDirection::Alternate);
    assert_eq!(anim.fill_mode, AnimationFillMode::Forwards);
}

#[test]
fn test_animation_individual_properties() {
    let css = r#"
        #button {
            animation-name: pulse;
            animation-duration: 2s;
            animation-timing-function: ease-in-out;
            animation-delay: 0.5s;
            animation-iteration-count: 3;
            animation-direction: reverse;
            animation-fill-mode: both;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    assert!(!result.has_errors());

    let style = result.stylesheet.get("button").unwrap();
    let anim = style.animation.as_ref().unwrap();
    assert_eq!(anim.name, "pulse");
    assert_eq!(anim.duration_ms, 2000);
    assert_eq!(anim.timing, AnimationTiming::EaseInOut);
    assert_eq!(anim.delay_ms, 500);
    assert_eq!(anim.iteration_count, 3);
    assert_eq!(anim.direction, AnimationDirection::Reverse);
    assert_eq!(anim.fill_mode, AnimationFillMode::Both);
}

#[test]
fn test_animation_with_keyframes() {
    let css = r#"
        @keyframes fade-in {
            from { opacity: 0; }
            to { opacity: 1; }
        }

        #card {
            animation: fade-in 300ms ease-out forwards;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    assert!(!result.has_errors());

    // Both keyframes and animation property should be parsed
    assert!(result.stylesheet.contains_keyframes("fade-in"));

    let style = result.stylesheet.get("card").unwrap();
    let anim = style.animation.as_ref().unwrap();
    assert_eq!(anim.name, "fade-in");
    assert_eq!(anim.duration_ms, 300);
    assert_eq!(anim.timing, AnimationTiming::EaseOut);
    assert_eq!(anim.fill_mode, AnimationFillMode::Forwards);
}

#[test]
fn test_resolve_animation() {
    let css = r#"
        @keyframes slide-in {
            from { opacity: 0; transform: translateY(20px); }
            to { opacity: 1; transform: translateY(0); }
        }

        #modal {
            animation: slide-in 500ms ease-out 100ms;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    assert!(!result.has_errors());

    // resolve_animation should combine keyframes and animation config
    let motion = result.stylesheet.resolve_animation("modal").unwrap();

    // Check duration comes from animation property
    assert_eq!(motion.enter_duration_ms, 500);
    assert_eq!(motion.exit_duration_ms, 500);
    assert_eq!(motion.enter_delay_ms, 100);

    // Check enter_from comes from first keyframe
    let enter_from = motion.enter_from.as_ref().unwrap();
    assert_eq!(enter_from.opacity, Some(0.0));
    assert_eq!(enter_from.translate_y, Some(20.0));

    // Check exit_to comes from last keyframe
    let exit_to = motion.exit_to.as_ref().unwrap();
    assert_eq!(exit_to.opacity, Some(1.0));
    assert_eq!(exit_to.translate_y, Some(0.0));
}

#[test]
fn test_resolve_animation_missing_keyframes() {
    let css = r#"
        #card {
            animation: nonexistent 300ms;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    // Should return None when keyframes don't exist
    assert!(result.stylesheet.resolve_animation("card").is_none());
}

#[test]
fn test_resolve_animation_no_animation_property() {
    let css = r#"
        @keyframes fade-in {
            from { opacity: 0; }
            to { opacity: 1; }
        }

        #card {
            background: blue;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    // Should return None when element has no animation property
    assert!(result.stylesheet.resolve_animation("card").is_none());
}

// =========================================================================
// Gradient Tests
// =========================================================================

#[test]
fn animation_picks_up_cubic_bezier_from_var_ease() {
    // Regression: `animation: name dur var(--ease-state);` must
    // resolve the var to its cubic-bezier literal AND survive
    // whitespace-tokenisation in parse_animation. Without paren-
    // aware splitting the cubic-bezier args get chopped into
    // separate tokens and the timing falls back to the default
    // `Ease`, defeating the whole semantic-easing wiring.
    let mut vars = std::collections::HashMap::new();
    vars.insert(
        "ease-state".to_string(),
        "cubic-bezier(0.25, 0.10, 0.25, 1.0)".to_string(),
    );
    vars.insert("duration-fast".to_string(), "180ms".to_string());

    let css = "#foo { animation: foo-enter var(--duration-fast) var(--ease-state); }";
    let sheet = Stylesheet::parse_with_variables(css, &vars).expect("parse");
    let style = sheet.get("foo").expect("rule");
    let anim = style.animation.as_ref().expect("animation set");
    assert_eq!(anim.name, "foo-enter");
    assert_eq!(anim.duration_ms, 180);
    match anim.timing {
        AnimationTiming::CubicBezier(a, b, c, d) => {
            assert!((a - 0.25).abs() < 1e-3 && (b - 0.10).abs() < 1e-3);
            assert!((c - 0.25).abs() < 1e-3 && (d - 1.0).abs() < 1e-3);
        }
        other => panic!("expected CubicBezier, got {:?}", other),
    }
}

#[test]
fn class_animation_picks_up_cubic_bezier_from_var_ease() {
    // Same as `animation_picks_up_cubic_bezier_from_var_ease`
    // but for a class selector and chained next to other
    // properties — the shape cn_styles actually uses.
    let mut vars = std::collections::HashMap::new();
    vars.insert(
        "ease-spring".to_string(),
        "cubic-bezier(0.34, 1.3, 0.64, 1)".to_string(),
    );
    vars.insert("duration-normal".to_string(), "240ms".to_string());

    let css = r#"
        .cn-popover-content {
            background: white;
            padding: 16px;
            animation: cn-popover-enter var(--duration-normal) var(--ease-spring);
            transform-origin: top center;
        }
    "#;
    let sheet = Stylesheet::parse_with_variables(css, &vars).expect("parse");
    let style = sheet
        .get_class("cn-popover-content")
        .expect("no class style registered");
    let anim = style
        .animation
        .as_ref()
        .expect("animation set on .cn-popover-content");
    assert_eq!(anim.name, "cn-popover-enter");
    assert_eq!(anim.duration_ms, 240);
    match anim.timing {
        AnimationTiming::CubicBezier(a, b, c, d) => {
            assert!((a - 0.34).abs() < 1e-3 && (b - 1.3).abs() < 1e-3);
            assert!((c - 0.64).abs() < 1e-3 && (d - 1.0).abs() < 1e-3);
        }
        other => panic!("expected CubicBezier, got {:?}", other),
    }
}

#[test]
fn transition_picks_up_cubic_bezier_from_var_ease() {
    let mut vars = std::collections::HashMap::new();
    vars.insert(
        "ease-state".to_string(),
        "cubic-bezier(0.25, 0.10, 0.25, 1.0)".to_string(),
    );
    vars.insert("duration-fast".to_string(), "150ms".to_string());

    let css = "#foo { transition: background var(--duration-fast) var(--ease-state), \
               border-color var(--duration-fast) var(--ease-state); }";
    let sheet = Stylesheet::parse_with_variables(css, &vars).expect("parse");
    let style = sheet.get("foo").expect("rule");
    let trans = style.transition.as_ref().expect("transitions set");
    assert_eq!(trans.transitions.len(), 2);
    for t in &trans.transitions {
        assert_eq!(t.duration_ms, 150);
        assert!(matches!(t.timing, AnimationTiming::CubicBezier(_, _, _, _)));
    }
}

/// CSS viewport units resolve against the live viewport.
///
/// The calc module has understood `vh` / `vw` all along, but this
/// parser only handled `px`, so `height: 100vh` parsed to nothing
/// and the declaration was dropped. A page relying on it for its
/// height then had none, and `overflow: scroll` had nothing to
/// scroll.
mod viewport_units {
    use super::*;

    fn with_viewport(w: f32, h: f32) {
        use blinc_core::context_state::{BlincContextState, HookState};
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex};
        static I: std::sync::Once = std::sync::Once::new();
        I.call_once(|| {
            if !BlincContextState::is_initialized() {
                BlincContextState::init(
                    blinc_core::reactive::global_graph(),
                    Arc::new(Mutex::new(HookState::new())),
                    Arc::new(AtomicBool::new(false)),
                );
            }
        });
        BlincContextState::get().set_viewport_size(w, h);
    }

    #[test]
    fn vh_and_vw_resolve() {
        with_viewport(1000.0, 800.0);
        assert_eq!(parse_css_px("100vh"), Some(800.0));
        assert_eq!(parse_css_px("50vw"), Some(500.0));
        assert_eq!(parse_css_px("25vh"), Some(200.0));
    }

    #[test]
    fn px_and_unitless_still_work() {
        with_viewport(1000.0, 800.0);
        assert_eq!(parse_css_px("12px"), Some(12.0));
        assert_eq!(parse_css_px("12"), Some(12.0));
    }
}
