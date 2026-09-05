//! Linear, radial and conic gradients.

use blinc_core::{Brush, Gradient};

use crate::parser::*;

#[test]
fn test_linear_gradient_angle() {
    let css = r#"#card { background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();
    assert!(style.background.is_some());

    if let Some(Brush::Gradient(Gradient::Linear { stops, .. })) = &style.background {
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0].offset, 0.0);
        assert_eq!(stops[1].offset, 1.0);
    } else {
        panic!("Expected linear gradient");
    }
}

#[test]
fn test_linear_gradient_to_right() {
    let css = r#"#card { background: linear-gradient(to right, red, blue); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Linear { start, end, .. })) = &style.background {
        // "to right" means 90deg, which should be start=(0, 0.5), end=(1, 0.5)
        assert!((start.x - 0.0).abs() < 0.01);
        assert!((start.y - 0.5).abs() < 0.01);
        assert!((end.x - 1.0).abs() < 0.01);
        assert!((end.y - 0.5).abs() < 0.01);
    } else {
        panic!("Expected linear gradient");
    }
}

#[test]
fn test_linear_gradient_to_bottom() {
    let css = r#"#card { background: linear-gradient(to bottom, #fff, #000); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Linear { start, end, .. })) = &style.background {
        // "to bottom" means 180deg, which should be start=(0.5, 0), end=(0.5, 1)
        assert!((start.x - 0.5).abs() < 0.01);
        assert!((start.y - 0.0).abs() < 0.01);
        assert!((end.x - 0.5).abs() < 0.01);
        assert!((end.y - 1.0).abs() < 0.01);
    } else {
        panic!("Expected linear gradient");
    }
}

#[test]
fn test_linear_gradient_to_bottom_right() {
    let css = r#"#card { background: linear-gradient(to bottom right, red, blue); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Linear { start, end, .. })) = &style.background {
        // "to bottom right" means 135deg, which should be start=(0, 0), end=(1, 1)
        assert!((start.x - 0.0).abs() < 0.01);
        assert!((start.y - 0.0).abs() < 0.01);
        assert!((end.x - 1.0).abs() < 0.01);
        assert!((end.y - 1.0).abs() < 0.01);
    } else {
        panic!("Expected linear gradient");
    }
}

#[test]
fn test_linear_gradient_multiple_stops() {
    let css = r#"#card { background: linear-gradient(90deg, red 0%, yellow 50%, green 100%); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Linear { stops, .. })) = &style.background {
        assert_eq!(stops.len(), 3);
        assert_eq!(stops[0].offset, 0.0);
        assert_eq!(stops[1].offset, 0.5);
        assert_eq!(stops[2].offset, 1.0);
    } else {
        panic!("Expected linear gradient with 3 stops");
    }
}

#[test]
fn test_linear_gradient_implied_positions() {
    let css = r#"#card { background: linear-gradient(to bottom, red, yellow, green); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Linear { stops, .. })) = &style.background {
        // With 3 stops and no explicit positions, should be 0%, 50%, 100%
        assert_eq!(stops.len(), 3);
        assert_eq!(stops[0].offset, 0.0);
        assert_eq!(stops[1].offset, 0.5);
        assert_eq!(stops[2].offset, 1.0);
    } else {
        panic!("Expected linear gradient with implied positions");
    }
}

#[test]
fn test_linear_gradient_rgba_colors() {
    let css = r#"#card { background: linear-gradient(45deg, rgba(255, 0, 0, 0.5) 0%, rgba(0, 0, 255, 0.8) 100%); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Linear { stops, .. })) = &style.background {
        assert_eq!(stops.len(), 2);
        // Check that RGBA colors were parsed (alpha should be < 1.0)
        assert!(stops[0].color.a < 1.0);
        assert!(stops[1].color.a < 1.0);
    } else {
        panic!("Expected linear gradient with RGBA colors");
    }
}

#[test]
fn test_linear_gradient_angle_units() {
    // Test various angle units
    let css_deg = r#"#a { background: linear-gradient(90deg, red, blue); }"#;
    let css_turn = r#"#b { background: linear-gradient(0.25turn, red, blue); }"#;
    let css_rad = r#"#c { background: linear-gradient(1.5708rad, red, blue); }"#;

    let result_deg = Stylesheet::parse_with_errors(css_deg);
    let result_turn = Stylesheet::parse_with_errors(css_turn);
    let result_rad = Stylesheet::parse_with_errors(css_rad);

    // All should parse to approximately the same gradient (90 degrees)
    if let (
        Some(Brush::Gradient(Gradient::Linear {
            start: s1, end: e1, ..
        })),
        Some(Brush::Gradient(Gradient::Linear {
            start: s2, end: e2, ..
        })),
        Some(Brush::Gradient(Gradient::Linear {
            start: s3, end: e3, ..
        })),
    ) = (
        &result_deg.stylesheet.get("a").unwrap().background,
        &result_turn.stylesheet.get("b").unwrap().background,
        &result_rad.stylesheet.get("c").unwrap().background,
    ) {
        // All should have similar start/end points (allowing for floating point)
        assert!((s1.x - s2.x).abs() < 0.1);
        assert!((e1.x - e2.x).abs() < 0.1);
        assert!((s1.x - s3.x).abs() < 0.1);
        assert!((e1.x - e3.x).abs() < 0.1);
    } else {
        panic!("Expected linear gradients");
    }
}

#[test]
fn test_radial_gradient_simple() {
    let css = r#"#card { background: radial-gradient(circle, red, blue); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Radial { stops, .. })) = &style.background {
        assert_eq!(stops.len(), 2);
    } else {
        panic!("Expected radial gradient");
    }
}

#[test]
fn test_radial_gradient_at_center() {
    let css = r#"#card { background: radial-gradient(circle at center, red, blue); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Radial { center, .. })) = &style.background {
        assert!((center.x - 0.5).abs() < 0.01);
        assert!((center.y - 0.5).abs() < 0.01);
    } else {
        panic!("Expected radial gradient");
    }
}

#[test]
fn test_radial_gradient_at_position() {
    let css = r#"#card { background: radial-gradient(circle at 25% 75%, red, blue); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Radial { center, .. })) = &style.background {
        assert!((center.x - 0.25).abs() < 0.01);
        assert!((center.y - 0.75).abs() < 0.01);
    } else {
        panic!("Expected radial gradient at custom position");
    }
}

#[test]
fn test_radial_gradient_multiple_stops() {
    let css = r#"#card { background: radial-gradient(circle, red 0%, yellow 50%, green 100%); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Radial { stops, .. })) = &style.background {
        assert_eq!(stops.len(), 3);
        assert_eq!(stops[0].offset, 0.0);
        assert_eq!(stops[1].offset, 0.5);
        assert_eq!(stops[2].offset, 1.0);
    } else {
        panic!("Expected radial gradient with 3 stops");
    }
}

#[test]
fn test_radial_gradient_ellipse() {
    let css = r#"#card { background: radial-gradient(ellipse at center, red, blue); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();
    assert!(matches!(
        &style.background,
        Some(Brush::Gradient(Gradient::Radial { .. }))
    ));
}

#[test]
fn test_conic_gradient_simple() {
    let css = r#"#card { background: conic-gradient(red, yellow, green, blue, red); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Conic { stops, .. })) = &style.background {
        assert_eq!(stops.len(), 5);
    } else {
        panic!("Expected conic gradient");
    }
}

#[test]
fn test_conic_gradient_from_angle() {
    let css = r#"#card { background: conic-gradient(from 45deg, red, blue); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Conic { start_angle, .. })) = &style.background {
        // 45 degrees in radians is approximately 0.785
        assert!((*start_angle - 0.785).abs() < 0.01);
    } else {
        panic!("Expected conic gradient with start angle");
    }
}

#[test]
fn test_conic_gradient_at_position() {
    let css = r#"#card { background: conic-gradient(at 25% 75%, red, blue); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Conic { center, .. })) = &style.background {
        assert!((center.x - 0.25).abs() < 0.01);
        assert!((center.y - 0.75).abs() < 0.01);
    } else {
        panic!("Expected conic gradient at custom position");
    }
}

#[test]
fn test_conic_gradient_from_at() {
    let css = r#"#card { background: conic-gradient(from 90deg at center, red, blue); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Conic {
        start_angle,
        center,
        ..
    })) = &style.background
    {
        // 90 degrees in radians is approximately 1.571
        assert!((*start_angle - 1.571).abs() < 0.01);
        assert!((center.x - 0.5).abs() < 0.01);
        assert!((center.y - 0.5).abs() < 0.01);
    } else {
        panic!("Expected conic gradient with angle and position");
    }
}

#[test]
fn test_gradient_with_css_variables() {
    let css = r#"
        :root {
            --start-color: #667eea;
            --end-color: #764ba2;
        }
        #card {
            background: linear-gradient(135deg, var(--start-color), var(--end-color));
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);

    // This test verifies that gradients work in the CSS context
    // Variable resolution happens at parse time
    let style = result.stylesheet.get("card").unwrap();
    assert!(style.background.is_some());
}

#[test]
fn test_gradient_fallback_to_solid() {
    // If gradient parsing fails, should fall through to color parsing
    let css = r#"#card { background: #FF0000; }"#;
    let result = Stylesheet::parse_with_errors(css);

    let style = result.stylesheet.get("card").unwrap();
    if let Some(Brush::Solid(color)) = &style.background {
        assert_eq!(color.r, 1.0);
        assert_eq!(color.g, 0.0);
        assert_eq!(color.b, 0.0);
    } else {
        panic!("Expected solid color");
    }
}

#[test]
fn test_gradient_with_named_colors() {
    let css = r#"#card { background: linear-gradient(to right, red, orange, yellow, green, blue, purple); }"#;
    let result = Stylesheet::parse_with_errors(css);

    assert!(!result.has_errors());
    let style = result.stylesheet.get("card").unwrap();

    if let Some(Brush::Gradient(Gradient::Linear { stops, .. })) = &style.background {
        assert_eq!(stops.len(), 6);
        // Check that positions are evenly distributed
        assert_eq!(stops[0].offset, 0.0);
        assert!((stops[1].offset - 0.2).abs() < 0.01);
        assert!((stops[2].offset - 0.4).abs() < 0.01);
        assert!((stops[3].offset - 0.6).abs() < 0.01);
        assert!((stops[4].offset - 0.8).abs() < 0.01);
        assert_eq!(stops[5].offset, 1.0);
    } else {
        panic!("Expected linear gradient with 6 named colors");
    }
}

// =========================================================================
// Length Unit Tests
// =========================================================================
