//! Lengths, viewport units, and `calc()` values.

use crate::parser::*;
use crate::units::Length;

#[test]
fn test_parse_css_length_px() {
    let len = parse_css_length("16px").unwrap();
    assert!(matches!(len, Length::Px(v) if (v - 16.0).abs() < 0.01));
    assert_eq!(len.to_px(), 16.0);
}

#[test]
fn test_parse_css_length_sp() {
    // sp = spacing units (4px grid)
    let len = parse_css_length("4sp").unwrap();
    assert!(matches!(len, Length::Sp(v) if (v - 4.0).abs() < 0.01));
    assert_eq!(len.to_px(), 16.0); // 4 * 4 = 16px
}

#[test]
fn test_parse_css_length_pct() {
    let len = parse_css_length("50%").unwrap();
    assert!(matches!(len, Length::Pct(v) if (v - 50.0).abs() < 0.01));
    // Percentage doesn't convert to pixels without context
    assert_eq!(len.to_px(), 0.0);
}

#[test]
fn test_parse_css_length_unitless() {
    // Unitless treated as pixels for backwards compatibility
    let len = parse_css_length("24").unwrap();
    assert!(matches!(len, Length::Px(v) if (v - 24.0).abs() < 0.01));
    assert_eq!(len.to_px(), 24.0);
}

#[test]
fn test_border_radius_with_sp() {
    let css = "#card { border-radius: 2sp; }"; // 2 * 4 = 8px
    let result = Stylesheet::parse_with_errors(css);

    let style = result.stylesheet.get("card").unwrap();
    if let Some(radius) = &style.corner_radius {
        // 2sp = 8px
        assert_eq!(radius.top_left, 8.0);
    } else {
        panic!("Expected corner radius to be parsed");
    }
}

#[test]
fn test_shadow_with_sp() {
    let css = "#card { box-shadow: 1sp 2sp 4sp rgba(0,0,0,0.3); }";
    let result = Stylesheet::parse_with_errors(css);

    let style = result.stylesheet.get("card").unwrap();
    if let Some(shadow) = style.shadow.first() {
        // 1sp = 4px, 2sp = 8px, 4sp = 16px
        assert_eq!(shadow.offset_x, 4.0);
        assert_eq!(shadow.offset_y, 8.0);
        assert_eq!(shadow.blur, 16.0);
    } else {
        panic!("Expected shadow to be parsed");
    }
}

#[test]
fn test_transform_with_sp() {
    use blinc_core::Transform;

    let css = "#card { transform: translate(4sp, 2sp); }";
    let result = Stylesheet::parse_with_errors(css);

    let style = result.stylesheet.get("card").unwrap();
    if let Some(Transform::Affine2D(affine)) = &style.transform {
        // 4sp = 16px, 2sp = 8px
        // elements[4] = tx, elements[5] = ty
        assert!((affine.elements[4] - 16.0).abs() < 0.01);
        assert!((affine.elements[5] - 8.0).abs() < 0.01);
    } else {
        panic!("Expected Affine2D transform to be parsed");
    }
}

#[test]
fn test_translatex_with_sp() {
    use blinc_core::Transform;

    let css = "#card { transform: translateX(4sp); }";
    let result = Stylesheet::parse_with_errors(css);

    let style = result.stylesheet.get("card").unwrap();
    if let Some(Transform::Affine2D(affine)) = &style.transform {
        assert!((affine.elements[4] - 16.0).abs() < 0.01); // 4sp = 16px
        assert!((affine.elements[5] - 0.0).abs() < 0.01);
    } else {
        panic!("Expected Affine2D transform to be parsed");
    }
}

#[test]
fn test_translatey_with_sp() {
    use blinc_core::Transform;

    let css = "#card { transform: translateY(2sp); }";
    let result = Stylesheet::parse_with_errors(css);

    let style = result.stylesheet.get("card").unwrap();
    if let Some(Transform::Affine2D(affine)) = &style.transform {
        assert!((affine.elements[4] - 0.0).abs() < 0.01);
        assert!((affine.elements[5] - 8.0).abs() < 0.01); // 2sp = 8px
    } else {
        panic!("Expected Affine2D transform to be parsed");
    }
}

// =========================================================================
// Comma-Separated Selector Tests
// =========================================================================

#[test]
fn test_calc_in_width() {
    let css = "#a { width: calc(100 - 20); }";
    let result = Stylesheet::parse_with_errors(css);
    let style = result.stylesheet.get("a").unwrap();
    assert_eq!(
        style.width,
        Some(crate::element_style::StyleDimension::Length(80.0))
    );
}

#[test]
fn test_calc_in_padding() {
    let css = "#a { padding: calc(8 * 2); }";
    let result = Stylesheet::parse_with_errors(css);
    let style = result.stylesheet.get("a").unwrap();
    assert_eq!(
        style.padding,
        Some(crate::element_style::SpacingRect::uniform(16.0))
    );
}

#[test]
fn test_calc_in_border_width() {
    let css = "#a { border-width: calc(1 + 1); }";
    let result = Stylesheet::parse_with_errors(css);
    let style = result.stylesheet.get("a").unwrap();
    assert_eq!(style.border_width, Some(2.0));
}

#[test]
fn test_calc_in_gap() {
    let css = "#a { gap: calc(4 * 3); }";
    let result = Stylesheet::parse_with_errors(css);
    let style = result.stylesheet.get("a").unwrap();
    assert_eq!(style.gap, Some(12.0));
}

// =====================================================================
// @flow parser tests
// =====================================================================
