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

#[cfg(test)]
mod inset_shadow_tests {
    use crate::element_style::ElementStyle;
    use crate::parser::Stylesheet;

    fn shadows(css: &str) -> Vec<blinc_core::Shadow> {
        let sheet = Stylesheet::parse(&format!("#x {{ {css} }}")).expect("css");
        let style: ElementStyle = sheet.get("x").cloned().expect("rule");
        style.shadow
    }

    /// `inset` is written first about as often as last, and CSS allows
    /// either, so both have to parse.
    #[test]
    fn inset_is_recognised_at_either_end() {
        let first = shadows("box-shadow: inset 0px 2px 4px #000000;");
        assert_eq!(first.len(), 1);
        assert!(first[0].inset, "`inset` leading was not picked up");

        let last = shadows("box-shadow: 0px 2px 4px #000000 inset;");
        assert_eq!(last.len(), 1);
        assert!(last[0].inset, "`inset` trailing was not picked up");

        // And the geometry survives either way.
        assert_eq!(first[0].offset_y, last[0].offset_y);
        assert_eq!(first[0].blur, last[0].blur);
    }

    /// Without the keyword it must stay an outer shadow.
    #[test]
    fn a_plain_shadow_is_not_inset() {
        let s = shadows("box-shadow: 0px 2px 4px #000000;");
        assert_eq!(s.len(), 1);
        assert!(!s[0].inset);
    }

    /// The four-value form carries a spread, and `inset` must not be
    /// mistaken for it.
    #[test]
    fn inset_coexists_with_spread() {
        let s = shadows("box-shadow: inset 0px 2px 4px 1px #000000;");
        assert_eq!(s.len(), 1);
        assert!(s[0].inset);
        assert_eq!(s[0].spread, 1.0);
        assert_eq!(s[0].blur, 4.0);
    }

    /// A stack can mix the two, and each layer keeps its own flag. This is
    /// the case the paint walk splits on.
    #[test]
    fn a_stack_can_mix_inset_and_outer_layers() {
        let s = shadows("box-shadow: 0px 4px 8px #111111, inset 0px 1px 2px #222222;");
        assert_eq!(s.len(), 2, "both layers should parse");
        assert!(!s[0].inset, "the first layer is an outer shadow");
        assert!(s[1].inset, "the second layer is inset");
    }
}
