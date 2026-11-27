//! Inset shadows set from Rust reach the element, whether by a builder, a style or a merge, and
//! the outer and inset lists are one `box-shadow` value.

use blinc_core::{Color, Shadow};
use blinc_layout::div::{Div, ElementBuilder, div};
use blinc_layout::element_style::ElementStyle;

fn shadow(blur: f32) -> Shadow {
    Shadow::new(0.0, 0.0, blur, Color::BLACK)
}

/// The blur radii of the outer and inset layers an element renders.
fn blurs(element: &Div) -> (Vec<f32>, Vec<f32>) {
    let props = element.render_props();
    let of = |layers: &[Shadow]| layers.iter().map(|s| s.blur).collect();
    (of(&props.shadow), of(&props.inner_shadow))
}

#[test]
fn a_builder_and_a_style_set_inset_shadows() {
    assert_eq!(blurs(&div().inset_shadow(shadow(3.0))), (vec![], vec![3.0]));
    assert_eq!(
        blurs(&div().shadow(shadow(1.0)).inset_shadow(shadow(2.0))),
        (vec![1.0], vec![2.0])
    );

    let style = ElementStyle::new()
        .shadow(shadow(4.0))
        .inset_shadow(shadow(5.0));
    let mut styled = div();
    styled.set_style(&style);
    assert_eq!(blurs(&styled), (vec![4.0], vec![5.0]));
}

#[test]
fn a_later_box_shadow_replaces_both_lists() {
    let mut element = div().shadow(shadow(1.0)).inset_shadow(shadow(2.0));
    element.merge(div().shadow(shadow(7.0)));
    assert_eq!(blurs(&element), (vec![7.0], vec![]));
    element.merge(div().inset_shadow(shadow(8.0)));
    assert_eq!(blurs(&element), (vec![], vec![8.0]));
    // A merge that sets no shadow keeps what is there.
    element.merge(div().w(10.0));
    assert_eq!(blurs(&element), (vec![], vec![8.0]));

    element.set_style(&ElementStyle::new().shadow(shadow(9.0)));
    assert_eq!(blurs(&element), (vec![9.0], vec![]));
}

#[test]
fn the_theme_inner_shadow_is_inset() {
    blinc_theme::ThemeState::init_default();
    let (outer, inset) = blurs(&div().shadow_inner());
    assert!(outer.is_empty() && !inset.is_empty());
}
