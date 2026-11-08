//! Runtime overrides reach every way of reading a token: the single-token
//! getter, the whole-set accessor and the CSS variable map.

use blinc_core::Color;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, RadiusToken, SpacingToken, ThemeState};

#[test]
fn overrides_reach_getters_accessors_and_css_variables() {
    ThemeState::init(HybridTheme::bundle(), ColorScheme::Light);
    let theme = ThemeState::get();
    let before = theme.to_css_variable_map();

    theme.set_radius_override(RadiusToken::Lg, 3.0);
    theme.set_spacing_override(SpacingToken::Space4, 99.0);
    theme.set_color_override(ColorToken::TextPrimary, Color::from_hex(0x0000ff));

    let vars = theme.to_css_variable_map();
    assert_eq!(theme.radius(RadiusToken::Lg), 3.0);
    assert_eq!(theme.radii().radius_lg, 3.0);
    assert_eq!(vars["radius-lg"], "3px");
    assert_eq!(theme.spacing_value(SpacingToken::Space4), 99.0);
    assert_eq!(theme.spacing().space_4, 99.0);
    assert_eq!(vars["space-4"], "99px");
    assert_eq!(
        theme.color(ColorToken::TextPrimary),
        Color::from_hex(0x0000ff)
    );
    assert_eq!(theme.colors().text_primary, Color::from_hex(0x0000ff));
    assert_eq!(vars["text-primary"], "#0000ff");
    // Tokens with no override keep the theme's value.
    assert_eq!(vars["radius-md"], before["radius-md"]);
    assert_eq!(vars["space-3"], before["space-3"]);

    theme.clear_overrides();
    assert_eq!(theme.to_css_variable_map(), before);
    assert_eq!(theme.radii().radius_lg, theme.radius(RadiusToken::Lg));
}
