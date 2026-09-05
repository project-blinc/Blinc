//! `@keyframes` parsing and lowering to animations.

use crate::parser::*;

#[test]
fn test_keyframes_basic() {
    let css = r#"
        @keyframes fade-in {
            from { opacity: 0; }
            to { opacity: 1; }
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    assert!(result.stylesheet.contains_keyframes("fade-in"));

    let keyframes = result.stylesheet.get_keyframes("fade-in").unwrap();
    assert_eq!(keyframes.name, "fade-in");
    assert_eq!(keyframes.keyframes.len(), 2);

    // Check first keyframe (from = 0%)
    assert_eq!(keyframes.keyframes[0].position, 0.0);
    assert_eq!(keyframes.keyframes[0].style.opacity, Some(0.0));

    // Check last keyframe (to = 100%)
    assert_eq!(keyframes.keyframes[1].position, 1.0);
    assert_eq!(keyframes.keyframes[1].style.opacity, Some(1.0));
}

#[test]
fn test_keyframes_percentage() {
    let css = r#"
        @keyframes pulse {
            0% { opacity: 1; }
            50% { opacity: 0.5; }
            100% { opacity: 1; }
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let keyframes = result.stylesheet.get_keyframes("pulse").unwrap();
    assert_eq!(keyframes.keyframes.len(), 3);

    assert_eq!(keyframes.keyframes[0].position, 0.0);
    assert_eq!(keyframes.keyframes[1].position, 0.5);
    assert_eq!(keyframes.keyframes[2].position, 1.0);
}

#[test]
fn test_keyframes_with_transform() {
    let css = r#"
        @keyframes slide-in {
            from {
                opacity: 0;
                transform: translateY(20px);
            }
            to {
                opacity: 1;
                transform: translateY(0);
            }
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let keyframes = result.stylesheet.get_keyframes("slide-in").unwrap();

    // First keyframe should have opacity 0 and transform
    assert_eq!(keyframes.keyframes[0].style.opacity, Some(0.0));
    assert!(keyframes.keyframes[0].style.transform.is_some());

    // Last keyframe should have opacity 1
    assert_eq!(keyframes.keyframes[1].style.opacity, Some(1.0));
    assert!(keyframes.keyframes[1].style.transform.is_some());
}

#[test]
fn test_keyframes_multiple_positions() {
    let css = r#"
        @keyframes blink {
            0%, 100% { opacity: 1; }
            50% { opacity: 0; }
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let keyframes = result.stylesheet.get_keyframes("blink").unwrap();

    // Should have 3 keyframes: 0%, 50%, 100%
    assert_eq!(keyframes.keyframes.len(), 3);

    // 0% and 100% should have opacity 1
    assert_eq!(keyframes.keyframes[0].position, 0.0);
    assert_eq!(keyframes.keyframes[0].style.opacity, Some(1.0));

    assert_eq!(keyframes.keyframes[1].position, 0.5);
    assert_eq!(keyframes.keyframes[1].style.opacity, Some(0.0));

    assert_eq!(keyframes.keyframes[2].position, 1.0);
    assert_eq!(keyframes.keyframes[2].style.opacity, Some(1.0));
}

#[test]
fn test_keyframes_count() {
    let css = r#"
        @keyframes anim1 {
            from { opacity: 0; }
            to { opacity: 1; }
        }
        @keyframes anim2 {
            from { opacity: 1; }
            to { opacity: 0; }
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    assert_eq!(result.stylesheet.keyframe_count(), 2);
    assert!(result.stylesheet.contains_keyframes("anim1"));
    assert!(result.stylesheet.contains_keyframes("anim2"));
}

#[test]
fn test_keyframes_names() {
    let css = r#"
        @keyframes fade-in { from { opacity: 0; } to { opacity: 1; } }
        @keyframes fade-out { from { opacity: 1; } to { opacity: 0; } }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    let names: Vec<_> = result.stylesheet.keyframe_names().collect();
    assert!(names.contains(&"fade-in"));
    assert!(names.contains(&"fade-out"));
}

#[test]
fn test_keyframes_to_motion_animation() {
    let css = r#"
        @keyframes fade-in {
            from { opacity: 0; }
            to { opacity: 1; }
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    let keyframes = result.stylesheet.get_keyframes("fade-in").unwrap();

    let motion = keyframes.to_motion_animation(300, 200);

    assert_eq!(motion.enter_duration_ms, 300);
    assert_eq!(motion.exit_duration_ms, 200);
    assert!(motion.enter_from.is_some());
    assert!(motion.exit_to.is_some());

    // enter_from should have opacity 0
    let enter = motion.enter_from.unwrap();
    assert_eq!(enter.opacity, Some(0.0));

    // exit_to should have opacity 1
    let exit = motion.exit_to.unwrap();
    assert_eq!(exit.opacity, Some(1.0));
}

#[test]
fn test_keyframes_to_multi_keyframe_animation() {
    use blinc_animation::Easing;

    let css = r#"
        @keyframes pulse {
            0% { opacity: 1; transform: scale(1); }
            50% { opacity: 0.8; transform: scale(1.05); }
            100% { opacity: 1; transform: scale(1); }
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    let keyframes = result.stylesheet.get_keyframes("pulse").unwrap();

    let animation = keyframes.to_multi_keyframe_animation(1000, Easing::EaseInOut);

    // Should have 3 keyframes
    assert_eq!(animation.keyframes().len(), 3);

    // Check keyframe positions
    assert_eq!(animation.keyframes()[0].time, 0.0);
    assert_eq!(animation.keyframes()[1].time, 0.5);
    assert_eq!(animation.keyframes()[2].time, 1.0);

    // Check opacity values
    assert_eq!(animation.keyframes()[0].properties.opacity, Some(1.0));
    assert_eq!(animation.keyframes()[1].properties.opacity, Some(0.8));
    assert_eq!(animation.keyframes()[2].properties.opacity, Some(1.0));
}

#[test]
fn test_keyframes_with_variables() {
    let css = r#"
        :root {
            --start-opacity: 0;
            --end-opacity: 1;
        }
        @keyframes fade-in {
            from { opacity: var(--start-opacity); }
            to { opacity: var(--end-opacity); }
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let keyframes = result.stylesheet.get_keyframes("fade-in").unwrap();

    // Variables should be resolved
    assert_eq!(keyframes.keyframes[0].style.opacity, Some(0.0));
    assert_eq!(keyframes.keyframes[1].style.opacity, Some(1.0));
}

#[test]
fn test_keyframes_mixed_with_rules() {
    let css = r#"
        @keyframes fade-in {
            from { opacity: 0; }
            to { opacity: 1; }
        }

        #card {
            background: #FF0000;
        }

        #card:hover {
            opacity: 0.9;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    // Keyframes should be parsed
    assert!(result.stylesheet.contains_keyframes("fade-in"));

    // Rules should also be parsed
    assert!(result.stylesheet.contains("card"));
    assert!(
        result
            .stylesheet
            .contains_with_state("card", ElementState::Hover)
    );
}

// =========================================================================
// Animation Property Tests
// =========================================================================
