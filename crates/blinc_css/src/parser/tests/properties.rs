//! Property aliases and the 3D shape shorthands.

use crate::parser::*;

#[test]
fn test_shape_alias() {
    let css = "#a { shape: sphere; }";
    let result = Stylesheet::parse_with_errors(css);
    let style = result.stylesheet.get("a").unwrap();
    assert_eq!(style.shape_3d.as_deref(), Some("sphere"));
}

#[test]
fn test_shape_3d_still_works() {
    let css = "#a { shape-3d: box; }";
    let result = Stylesheet::parse_with_errors(css);
    let style = result.stylesheet.get("a").unwrap();
    assert_eq!(style.shape_3d.as_deref(), Some("box"));
}

#[test]
fn test_shape_combine_alias() {
    let css = "#a { shape-combine: smooth-union; }";
    let result = Stylesheet::parse_with_errors(css);
    let style = result.stylesheet.get("a").unwrap();
    assert_eq!(style.op_3d.as_deref(), Some("smooth-union"));
}

#[test]
fn test_shape_blend_alias() {
    let css = "#a { shape-blend: 8; }";
    let result = Stylesheet::parse_with_errors(css);
    let style = result.stylesheet.get("a").unwrap();
    assert_eq!(style.blend_3d, Some(8.0));
}

#[test]
fn test_light_alias() {
    let css = "#a { light: 0.3 -0.8 0.5; }";
    let result = Stylesheet::parse_with_errors(css);
    let style = result.stylesheet.get("a").unwrap();
    let dir = style.light_direction.unwrap();
    assert!((dir[0] - 0.3).abs() < 0.01);
    assert!((dir[1] - (-0.8)).abs() < 0.01);
    assert!((dir[2] - 0.5).abs() < 0.01);
}

#[test]
fn test_surface_glass_alias() {
    let css = "#a { surface: glossy; }";
    let result = Stylesheet::parse_with_errors(css);
    let style = result.stylesheet.get("a").unwrap();
    assert!(matches!(
        style.material,
        Some(crate::material::Material::Glass(_))
    ));
}

#[test]
fn test_surface_metallic_alias() {
    let css = "#a { surface: chrome; }";
    let result = Stylesheet::parse_with_errors(css);
    let style = result.stylesheet.get("a").unwrap();
    assert!(matches!(
        style.material,
        Some(crate::material::Material::Metallic(_))
    ));
}

#[test]
fn test_surface_gold_alias() {
    let css = "#a { surface: gold; }";
    let result = Stylesheet::parse_with_errors(css);
    let style = result.stylesheet.get("a").unwrap();
    assert!(matches!(
        style.material,
        Some(crate::material::Material::Metallic(_))
    ));
}

// ====================================================================
// calc() in CSS values
// ====================================================================
