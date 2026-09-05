//! `ElementStyle` builder, merge, and macro tests.

use blinc_core::{Brush, Color, PointerEvents, Shadow, Transform};
use blinc_theme::ThemeState;

use crate::element_style::*;
use crate::{css, style};

#[test]
fn test_style_builder() {
    // Initialize theme (required for shadow_md which uses theme)
    ThemeState::init_default();

    let s = style().bg(Color::BLUE).rounded(8.0).shadow_md().scale(1.05);

    assert!(s.background.is_some());
    assert!(s.corner_radius.is_some());
    assert!(!s.shadow.is_empty());
    assert!(s.transform.is_some());
}

#[test]
fn test_style_merge() {
    // Initialize theme (required for shadow_sm which uses theme)
    ThemeState::init_default();

    let base = style().bg(Color::BLUE).rounded(8.0).shadow_sm();

    let hover = style().bg(Color::GREEN).scale(1.02);

    let merged = base.merge(&hover);

    // Background should be overridden
    assert!(matches!(merged.background, Some(Brush::Solid(c)) if c == Color::GREEN));
    // Corner radius should be preserved from base
    assert!(merged.corner_radius.is_some());
    // Shadow should be preserved from base
    assert!(!merged.shadow.is_empty());
    // Transform should come from hover
    assert!(merged.transform.is_some());
}

#[test]
fn test_style_empty() {
    let empty = ElementStyle::new();
    assert!(empty.is_empty());

    let non_empty = style().bg(Color::RED);
    assert!(!non_empty.is_empty());
}

// =========================================================================
// style! macro tests
// =========================================================================

#[test]
fn test_style_macro_empty() {
    let s = style!();
    assert!(s.is_empty());
}

#[test]
fn test_style_macro_basic() {
    ThemeState::init_default();

    let s = style! {
        bg: Color::BLUE,
        rounded: 8.0,
        opacity: 0.9,
    };

    assert!(matches!(s.background, Some(Brush::Solid(c)) if c == Color::BLUE));
    assert!(s.corner_radius.is_some());
    assert_eq!(s.opacity, Some(0.9));
}

#[test]
fn test_style_macro_presets() {
    ThemeState::init_default();

    let s = style! {
        bg: Color::WHITE,
        rounded_lg,
        shadow_md,
    };

    assert!(s.background.is_some());
    assert!(s.corner_radius.is_some());
    assert!(!s.shadow.is_empty());
}

#[test]
fn test_style_macro_transforms() {
    let s = style! {
        scale: 1.05,
    };
    assert!(s.transform.is_some());

    let s2 = style! {
        translate: (10.0, 20.0),
    };
    assert!(s2.transform.is_some());

    let s3 = style! {
        rotate_deg: 45.0,
    };
    assert!(s3.transform.is_some());

    let s4 = style! {
        scale_xy: (1.1, 0.9),
    };
    assert!(s4.transform.is_some());
}

#[test]
fn test_style_macro_materials() {
    let s = style! {
        glass,
        rounded: 16.0,
    };

    assert!(s.material.is_some());
    assert!(s.corner_radius.is_some());
    // Glass sets render layer to Glass
    assert!(s.render_layer.is_some());
}

#[test]
fn test_style_macro_opacity_presets() {
    let s1 = style! { opaque };
    assert_eq!(s1.opacity, Some(1.0));

    let s2 = style! { translucent };
    assert_eq!(s2.opacity, Some(0.5));

    let s3 = style! { transparent };
    assert_eq!(s3.opacity, Some(0.0));
}

#[test]
fn test_style_macro_combined() {
    ThemeState::init_default();

    // Test combining multiple properties
    let card_style = style! {
        bg: Color::WHITE,
        rounded_lg,
        shadow_md,
        opacity: 0.95,
        scale: 1.0,
    };

    assert!(card_style.background.is_some());
    assert!(card_style.corner_radius.is_some());
    assert!(!card_style.shadow.is_empty());
    assert_eq!(card_style.opacity, Some(0.95));
    assert!(card_style.transform.is_some());
}

#[test]
fn test_style_macro_rounded_variants() {
    ThemeState::init_default();

    let s1 = style! { rounded_sm };
    assert!(s1.corner_radius.is_some());

    let s2 = style! { rounded_md };
    assert!(s2.corner_radius.is_some());

    let s3 = style! { rounded_xl };
    assert!(s3.corner_radius.is_some());

    let s4 = style! { rounded_full };
    assert!(s4.corner_radius.is_some());

    let s5 = style! { rounded_none };
    assert!(s5.corner_radius.is_some());
}

#[test]
fn test_style_macro_shadow_variants() {
    ThemeState::init_default();

    let s1 = style! { shadow_sm };
    assert!(!s1.shadow.is_empty());

    let s2 = style! { shadow_lg };
    assert!(!s2.shadow.is_empty());

    let s3 = style! { shadow_xl };
    assert!(!s3.shadow.is_empty());

    let s4 = style! { shadow_none };
    assert!(!s4.shadow.is_empty()); // shadow_none sets a transparent shadow
}

#[test]
fn test_style_macro_material_variants() {
    let s1 = style! { metallic };
    assert!(s1.material.is_some());

    let s2 = style! { chrome };
    assert!(s2.material.is_some());

    let s3 = style! { gold };
    assert!(s3.material.is_some());

    let s4 = style! { wood };
    assert!(s4.material.is_some());
}

#[test]
fn test_style_macro_layer() {
    let s1 = style! { foreground };
    assert!(s1.render_layer.is_some());

    let s2 = style! { layer_background };
    assert!(s2.render_layer.is_some());
}

#[test]
fn test_style_macro_rounded_corners() {
    let s = style! {
        rounded_corners: (8.0, 8.0, 0.0, 0.0),
    };
    assert!(s.corner_radius.is_some());
    let cr = s.corner_radius.unwrap();
    assert_eq!(cr.top_left, 8.0);
    assert_eq!(cr.top_right, 8.0);
    assert_eq!(cr.bottom_right, 0.0);
    assert_eq!(cr.bottom_left, 0.0);
}

// =========================================================================
// css! macro tests - CSS property name compatibility
// =========================================================================

#[test]
fn test_css_macro_empty() {
    let s = css!();
    assert!(s.is_empty());
}

#[test]
fn test_css_macro_basic() {
    // Uses CSS property names with semicolon separators
    let s = css! {
        background: Color::BLUE;
        border-radius: 8.0;
        opacity: 0.9;
    };

    assert!(matches!(s.background, Some(Brush::Solid(c)) if c == Color::BLUE));
    assert!(s.corner_radius.is_some());
    assert_eq!(s.opacity, Some(0.9));
}

#[test]
fn test_css_macro_shadow() {
    ThemeState::init_default();

    let s = css! {
        box-shadow: md;
    };
    assert!(!s.shadow.is_empty());

    let s2 = css! {
        box-shadow: Shadow::new(0.0, 4.0, 8.0, Color::BLACK);
    };
    assert!(!s2.shadow.is_empty());
}

#[test]
fn test_css_macro_transform() {
    let s = css! {
        transform: Transform::scale(1.05, 1.05);
    };
    assert!(s.transform.is_some());
}

#[test]
fn test_css_macro_backdrop_filter() {
    // Blinc extension for materials
    let s = css! {
        backdrop-filter: glass;
    };
    assert!(s.material.is_some());
    assert!(s.render_layer.is_some()); // Glass sets render layer
}

#[test]
fn test_css_macro_combined() {
    ThemeState::init_default();

    // Full CSS-like card style
    let card = css! {
        background: Color::WHITE;
        border-radius: 12.0;
        box-shadow: lg;
        opacity: 0.95;
    };

    assert!(card.background.is_some());
    assert!(card.corner_radius.is_some());
    assert!(!card.shadow.is_empty());
    assert_eq!(card.opacity, Some(0.95));
}

#[test]
fn test_css_macro_animation() {
    let s = css! {
        animation-name: "fade-in";
        animation-duration: 300;
    };

    assert!(s.animation.is_some());
    let anim = s.animation.unwrap();
    assert_eq!(anim.name, "fade-in");
    assert_eq!(anim.duration_ms, 300);
}

#[test]
fn test_css_and_style_macros_produce_same_result() {
    // Both macros should produce equivalent ElementStyle for same properties
    let from_css = css! {
        background: Color::RED;
        border-radius: 10.0;
        opacity: 0.8;
    };

    let from_style = style! {
        bg: Color::RED,
        rounded: 10.0,
        opacity: 0.8,
    };

    // Same background
    assert!(matches!(from_css.background, Some(Brush::Solid(c)) if c == Color::RED));
    assert!(matches!(from_style.background, Some(Brush::Solid(c)) if c == Color::RED));

    // Same corner radius
    assert_eq!(from_css.corner_radius, from_style.corner_radius);

    // Same opacity
    assert_eq!(from_css.opacity, from_style.opacity);
}

#[test]
fn test_css_macro_text_properties() {
    let s = css! {
        color: Color::RED;
        font-size: 16.0;
        line-height: 1.5;
        letter-spacing: 0.5;
    };
    assert_eq!(s.text_color, Some(Color::RED));
    assert_eq!(s.font_size, Some(16.0));
    assert_eq!(s.line_height, Some(1.5));
    assert_eq!(s.letter_spacing, Some(0.5));
}

#[test]
fn test_style_macro_text_properties() {
    let s = style! {
        text_color: Color::BLUE,
        font_size: 14.0,
        line_height: 1.2,
        letter_spacing: 1.0,
    };
    assert_eq!(s.text_color, Some(Color::BLUE));
    assert_eq!(s.font_size, Some(14.0));
    assert_eq!(s.line_height, Some(1.2));
    assert_eq!(s.letter_spacing, Some(1.0));
}

#[test]
fn test_css_macro_text_decoration() {
    let s = css! {
        text-decoration: TextDecoration::Underline;
        text-decoration-color: Color::RED;
        text-decoration-thickness: 2.0;
    };
    assert_eq!(s.text_decoration, Some(TextDecoration::Underline));
    assert_eq!(s.text_decoration_color, Some(Color::RED));
    assert_eq!(s.text_decoration_thickness, Some(2.0));
}

#[test]
fn test_css_macro_text_overflow() {
    let s = css! {
        text-overflow: ellipsis;
        white-space: nowrap;
    };
    assert_eq!(s.text_overflow, Some(TextOverflow::Ellipsis));
    assert_eq!(s.white_space, Some(WhiteSpace::Nowrap));
}

#[test]
fn test_css_macro_position_inset() {
    let s = css! {
        position: absolute;
        top: 10.0;
        right: 20.0;
        bottom: 30.0;
        left: 40.0;
        z-index: 5;
    };
    assert_eq!(s.position, Some(StylePosition::Absolute));
    assert_eq!(s.top, Some(10.0));
    assert_eq!(s.right, Some(20.0));
    assert_eq!(s.bottom, Some(30.0));
    assert_eq!(s.left, Some(40.0));
    assert_eq!(s.z_index, Some(5));
}

#[test]
fn test_style_macro_position_inset() {
    let s = style! {
        position: StylePosition::Relative,
        top: 5.0,
        inset: 0.0,
        z_index: 10,
    };
    assert_eq!(s.position, Some(StylePosition::Relative));
    // inset overrides top
    assert_eq!(s.top, Some(0.0));
    assert_eq!(s.right, Some(0.0));
    assert_eq!(s.bottom, Some(0.0));
    assert_eq!(s.left, Some(0.0));
    assert_eq!(s.z_index, Some(10));
}

#[test]
fn test_css_macro_visibility() {
    let s = css! {
        visibility: hidden;
    };
    assert_eq!(s.visibility, Some(StyleVisibility::Hidden));
}

#[test]
fn test_css_macro_overflow_axes() {
    let s = css! {
        overflow-x: scroll;
        overflow-y: hidden;
    };
    assert_eq!(s.overflow_x, Some(StyleOverflow::Scroll));
    assert_eq!(s.overflow_y, Some(StyleOverflow::Clip));
}

#[test]
fn test_css_macro_outline() {
    let s = css! {
        outline: (2.0, Color::RED);
        outline-offset: 4.0;
    };
    assert_eq!(s.outline_width, Some(2.0));
    assert_eq!(s.outline_color, Some(Color::RED));
    assert_eq!(s.outline_offset, Some(4.0));
}

#[test]
fn test_style_macro_outline() {
    let s = style! {
        outline: (3.0, Color::BLUE),
        outline_offset: 2.0,
    };
    assert_eq!(s.outline_width, Some(3.0));
    assert_eq!(s.outline_color, Some(Color::BLUE));
    assert_eq!(s.outline_offset, Some(2.0));
}

#[test]
fn test_css_macro_form_colors() {
    let s = css! {
        caret-color: Color::RED;
        selection-color: Color::BLUE;
        placeholder-color: Color::rgba(0.5, 0.5, 0.5, 1.0);
        accent-color: Color::GREEN;
    };
    assert_eq!(s.caret_color, Some(Color::RED));
    assert_eq!(s.selection_color, Some(Color::BLUE));
    assert!(s.placeholder_color.is_some());
    assert_eq!(s.accent_color, Some(Color::GREEN));
}

#[test]
fn test_style_macro_form_colors() {
    let s = style! {
        caret_color: Color::RED,
        accent_color: Color::GREEN,
    };
    assert_eq!(s.caret_color, Some(Color::RED));
    assert_eq!(s.accent_color, Some(Color::GREEN));
}

#[test]
fn test_css_macro_svg_properties() {
    let s = css! {
        fill: Color::RED;
        stroke: Color::BLUE;
        stroke-width: 2.0;
        stroke-dashoffset: 10.0;
    };
    assert_eq!(s.fill, Some(Color::RED));
    assert_eq!(s.stroke, Some(Color::BLUE));
    assert_eq!(s.stroke_width, Some(2.0));
    assert_eq!(s.stroke_dashoffset, Some(10.0));
}

#[test]
fn test_style_macro_svg_properties() {
    let s = style! {
        fill: Color::RED,
        stroke: Color::BLUE,
        stroke_width: 3.0,
        stroke_dasharray: vec![5.0, 3.0],
        stroke_dashoffset: 0.0,
    };
    assert_eq!(s.fill, Some(Color::RED));
    assert_eq!(s.stroke, Some(Color::BLUE));
    assert_eq!(s.stroke_width, Some(3.0));
    assert_eq!(s.stroke_dasharray, Some(vec![5.0, 3.0]));
    assert_eq!(s.stroke_dashoffset, Some(0.0));
}

#[test]
fn test_css_macro_transform_extras() {
    let s = css! {
        transform-origin: (0.0, 100.0);
    };
    assert_eq!(s.transform_origin, Some([0.0, 100.0]));
}

#[test]
fn test_style_macro_transform_extras() {
    let s = style! {
        skew_x: 15.0,
        skew_y: 10.0,
        transform_origin: (50.0, 50.0),
    };
    assert_eq!(s.skew_x, Some(15.0));
    assert_eq!(s.skew_y, Some(10.0));
    assert_eq!(s.transform_origin, Some([50.0, 50.0]));
}

#[test]
fn test_css_macro_scrollbar() {
    let s = css! {
        scrollbar-color: (Color::RED, Color::WHITE);
        scrollbar-width: thin;
    };
    assert_eq!(s.scrollbar_color, Some((Color::RED, Color::WHITE)));
    assert_eq!(s.scrollbar_width, Some(ScrollbarWidth::Thin));
}

#[test]
fn test_css_macro_image_properties() {
    let s = css! {
        object-fit: 1;
        object-position: (0.5, 0.0);
    };
    assert_eq!(s.object_fit, Some(1));
    assert_eq!(s.object_position, Some([0.5, 0.0]));
}

#[test]
fn test_style_macro_image_properties() {
    let s = style! {
        object_fit: 0,
        object_position: (0.0, 1.0),
    };
    assert_eq!(s.object_fit, Some(0));
    assert_eq!(s.object_position, Some([0.0, 1.0]));
}

#[test]
fn test_css_macro_filter() {
    let f = CssFilter {
        grayscale: 1.0,
        ..Default::default()
    };
    let s = css! {
        filter: f;
    };
    assert!(s.filter.is_some());
    assert_eq!(s.filter.unwrap().grayscale, 1.0);
}

#[test]
fn test_style_macro_filter() {
    let f = CssFilter {
        brightness: 1.5,
        ..Default::default()
    };
    let s = style! {
        filter: f,
    };
    assert!(s.filter.is_some());
    assert_eq!(s.filter.unwrap().brightness, 1.5);
}

#[test]
fn test_css_macro_border_shorthand() {
    let s = css! {
        border: (2.0, Color::RED);
    };
    assert_eq!(s.border_width, Some(2.0));
    assert_eq!(s.border_color, Some(Color::RED));
}

#[test]
fn test_css_macro_display_block() {
    let s = css! {
        display: block;
    };
    assert_eq!(s.display, Some(StyleDisplay::Block));
}

#[test]
fn test_css_macro_pointer_events() {
    let s = css! {
        pointer-events: none;
    };
    assert_eq!(s.pointer_events, Some(PointerEvents::None));
}

#[test]
fn test_style_macro_pointer_events() {
    let s = style! {
        pointer_events_none,
    };
    assert_eq!(s.pointer_events, Some(PointerEvents::None));
}

#[test]
fn test_css_macro_inset() {
    let s = css! {
        inset: 10.0;
    };
    assert_eq!(s.top, Some(10.0));
    assert_eq!(s.right, Some(10.0));
    assert_eq!(s.bottom, Some(10.0));
    assert_eq!(s.left, Some(10.0));
}

#[test]
fn test_css_macro_flex_extras() {
    let s = css! {
        flex-wrap: nowrap;
        align-items: baseline;
        align-self: stretch;
    };
    assert_eq!(s.flex_wrap, Some(false));
    assert_eq!(s.align_items, Some(StyleAlign::Baseline));
    assert_eq!(s.align_self, Some(StyleAlign::Stretch));
}

#[test]
fn test_css_style_parity_text() {
    let from_css = css! {
        color: Color::RED;
        font-size: 16.0;
        letter-spacing: 2.0;
    };
    let from_style = style! {
        text_color: Color::RED,
        font_size: 16.0,
        letter_spacing: 2.0,
    };
    assert_eq!(from_css.text_color, from_style.text_color);
    assert_eq!(from_css.font_size, from_style.font_size);
    assert_eq!(from_css.letter_spacing, from_style.letter_spacing);
}

#[test]
fn test_css_style_parity_position() {
    let from_css = css! {
        position: absolute;
        top: 10.0;
        z-index: 5;
    };
    let from_style = style! {
        position: StylePosition::Absolute,
        top: 10.0,
        z_index: 5,
    };
    assert_eq!(from_css.position, from_style.position);
    assert_eq!(from_css.top, from_style.top);
    assert_eq!(from_css.z_index, from_style.z_index);
}
