#![cfg(feature = "headless")]
//! What an open overlay keeps the window drawing for: an animation that will end, not one that
//! repeats with no end elsewhere on the page.

use blinc_core::Color;
use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const CSS: &str = "
@keyframes spin { from { opacity: 1; } to { opacity: 0.3; } }
.forever { animation: spin 400ms linear infinite alternate; }
.once { animation: spin 300ms linear 1; }
";

fn page(classes: &'static [&'static str]) -> Option<Headless> {
    if !Headless::available() {
        return None;
    }
    let mut ui = Headless::new(200, 120).ok()?;
    ui.theme(HybridTheme::bundle().with_css(CSS), ColorScheme::Dark);
    ui.mount(move |_| {
        classes.iter().fold(div().w(200.0).h(120.0), |page, class| {
            page.child(
                div()
                    .class(*class)
                    .w(40.0)
                    .h(20.0)
                    .bg(Color::rgb(0.2, 0.4, 0.8)),
            )
        })
    });
    ui.frame(16);
    ui.frame(16);
    Some(ui)
}

#[test]
fn an_animation_that_repeats_forever_does_not_keep_an_overlay_drawing() {
    let Some(ui) = page(&["forever"]) else {
        return;
    };
    assert!(ui.tree().css_has_active(), "the animation is not playing");
    assert!(
        !ui.tree().css_has_active_for_overlay(),
        "an infinite animation on the page keeps an open overlay drawing"
    );
}

#[test]
fn an_animation_that_will_end_keeps_an_overlay_drawing_until_it_has() {
    let Some(mut ui) = page(&["once"]) else {
        return;
    };
    assert!(ui.tree().css_has_active_for_overlay());
    ui.advance(600);
    ui.frame(16);
    assert!(
        !ui.tree().css_has_active_for_overlay(),
        "a finished animation keeps an open overlay drawing"
    );
}

#[test]
fn a_finite_animation_beside_an_infinite_one_still_counts() {
    let Some(ui) = page(&["forever", "once"]) else {
        return;
    };
    assert!(ui.tree().css_has_active_for_overlay());
}
