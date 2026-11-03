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

    /// `--shadow-inner` is inset in every theme, so it must route to the inner list.
    #[test]
    fn the_inner_shadow_variable_routes_to_the_inner_list() {
        let value = theme_shadow("shadow-inner");
        let s = parse(&format!("box-shadow: {value};"));
        assert!(!s.inner_shadow.is_empty(), "shadow-inner produced no layer");
        assert!(s.shadow.is_empty(), "shadow-inner is not an outer shadow");
    }

    /// The `box-shadow` string a theme publishes for a token.
    fn theme_shadow(token: &str) -> String {
        blinc_theme::ThemeState::init_default();
        blinc_theme::ThemeState::get().to_css_variable_map()[token].clone()
    }

    /// Every shadow variable of every theme reads back as the stack it was made from: the same
    /// layers, offsets, blur, spread and colour (to the channel the 8-bit text keeps).
    #[test]
    fn a_themes_shadow_variables_read_back_as_its_stacks() {
        use blinc_theme::{ColorScheme, HybridTheme, ThemeState};
        for scheme in [ColorScheme::Light, ColorScheme::Dark] {
            ThemeState::init(HybridTheme::bundle(), scheme);
            let theme = ThemeState::get();
            let vars = theme.to_css_variable_map();
            let shadows = theme.shadows();
            for (name, stack) in [
                ("shadow-sm", &shadows.shadow_sm),
                ("shadow-default", &shadows.shadow_default),
                ("shadow-md", &shadows.shadow_md),
                ("shadow-lg", &shadows.shadow_lg),
                ("shadow-xl", &shadows.shadow_xl),
                ("shadow-2xl", &shadows.shadow_2xl),
                ("shadow-inner", &shadows.shadow_inner),
            ] {
                let s = parse(&format!("box-shadow: {};", vars[name]));
                let (outer, inner): (Vec<_>, Vec<_>) = stack.iter().partition(|l| !l.inset);
                assert_eq!(
                    s.shadow.len(),
                    outer.len(),
                    "{name} outer layers: {}",
                    vars[name]
                );
                assert_eq!(s.inner_shadow.len(), inner.len(), "{name} inner layers");
                for (got, want) in s
                    .shadow
                    .iter()
                    .zip(outer)
                    .chain(s.inner_shadow.iter().zip(inner))
                {
                    assert_eq!(
                        [got.offset_x, got.offset_y, got.blur, got.spread],
                        [want.offset_x, want.offset_y, want.blur, want.spread],
                        "{name}: {}",
                        vars[name]
                    );
                    let c = got.color;
                    for (a, b) in [
                        (c.r, want.color.r),
                        (c.g, want.color.g),
                        (c.b, want.color.b),
                    ] {
                        assert!((a - b).abs() <= 1.0 / 255.0 + 1e-6, "{name}: {a} vs {b}");
                    }
                    assert!((c.a - want.color.a).abs() < 1e-3, "{name} alpha");
                }
            }
            assert_eq!(vars["shadow-none"], "none");
        }
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
