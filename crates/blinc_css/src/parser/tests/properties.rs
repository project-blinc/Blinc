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

    fn parse(css: &str) -> ElementStyle {
        let sheet = Stylesheet::parse(&format!("#x {{ {css} }}")).expect("css");
        sheet.get("x").cloned().expect("rule")
    }

    /// `inset` is written first about as often as last, and CSS allows
    /// either, so both have to parse — and both land in the inner list.
    #[test]
    fn inset_is_recognised_at_either_end() {
        let first = parse("box-shadow: inset 0px 2px 4px #000000;");
        let last = parse("box-shadow: 0px 2px 4px #000000 inset;");

        assert_eq!(first.inner_shadow.len(), 1);
        assert!(
            first.shadow.is_empty(),
            "an inset layer is not an outer one"
        );
        assert_eq!(last.inner_shadow.len(), 1);
        assert!(last.shadow.is_empty());

        // And the geometry survives either way.
        assert_eq!(
            first.inner_shadow[0].offset_y,
            last.inner_shadow[0].offset_y
        );
        assert_eq!(first.inner_shadow[0].blur, last.inner_shadow[0].blur);
    }

    /// Without the keyword it must stay an outer shadow.
    #[test]
    fn a_plain_shadow_is_not_inset() {
        let s = parse("box-shadow: 0px 2px 4px #000000;");
        assert_eq!(s.shadow.len(), 1);
        assert!(s.inner_shadow.is_empty());
    }

    /// The four-value form carries a spread, and `inset` must not be
    /// mistaken for it.
    #[test]
    fn inset_coexists_with_spread() {
        let s = parse("box-shadow: inset 0px 2px 4px 1px #000000;");
        assert_eq!(s.inner_shadow.len(), 1);
        assert_eq!(s.inner_shadow[0].spread, 1.0);
        assert_eq!(s.inner_shadow[0].blur, 4.0);
    }

    /// A stack can mix the two. This is the split the paint walk relies
    /// on: outer layers before the fill, inner ones after.
    #[test]
    fn a_stack_splits_into_the_two_lists() {
        let s = parse("box-shadow: 0px 4px 8px #111111, inset 0px 1px 2px #222222;");
        assert_eq!(s.shadow.len(), 1, "one outer layer");
        assert_eq!(s.inner_shadow.len(), 1, "one inner layer");
        // Each layer kept its own geometry rather than being swapped.
        assert_eq!(s.shadow[0].blur, 8.0);
        assert_eq!(s.inner_shadow[0].blur, 2.0);
    }

    /// `theme(shadow-inner)` is the token form, and the token is inset in
    /// every theme, so it must route to the inner list.
    #[test]
    fn the_inner_theme_token_routes_to_the_inner_list() {
        let s = parse("box-shadow: theme(shadow-inner);");
        assert!(!s.inner_shadow.is_empty(), "shadow-inner produced no layer");
        assert!(s.shadow.is_empty(), "shadow-inner is not an outer shadow");
    }

    /// `text-shadow` takes no `inset`, so such a layer yields nothing
    /// rather than silently painting as a drop shadow.
    #[test]
    fn text_shadow_ignores_an_inset_layer() {
        let s = parse("text-shadow: inset 0px 1px 2px #000000;");
        assert!(s.text_shadow.is_none());

        let ok = parse("text-shadow: 0px 1px 2px #000000;");
        assert!(ok.text_shadow.is_some());
    }
}
