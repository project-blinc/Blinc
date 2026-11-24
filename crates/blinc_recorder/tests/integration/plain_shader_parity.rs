#![cfg(feature = "headless")]
//! The compact shader for plain primitives draws what the core shader draws.
//!
//! The same elements are rendered twice. In the second render a CSS mask that does not change a
//! pixel (black to black) keeps them out of the plain category, so the core shader draws them.

use blinc_core::Color;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ExpressiveTheme, HybridTheme, ThemeBundle};

const W: u32 = 260;
const H: u32 = 160;

fn scene() -> impl blinc_layout::prelude::ElementBuilder {
    let edge = Color::rgba(0.2, 0.25, 0.35, 0.6);
    div()
        .w(W as f32)
        .h(H as f32)
        .bg(Color::rgba(0.96, 0.97, 0.99, 1.0))
        .flex_row()
        .flex_wrap()
        .gap(3.0)
        .p(3.0)
        // A bordered, rounded card.
        .child(
            div()
                .class("t")
                .w(80.0)
                .h(50.0)
                .rounded(12.0)
                .bg(Color::rgba(1.0, 1.0, 1.0, 1.0))
                .border(1.0, edge),
        )
        // A circle.
        .child(
            div()
                .class("t")
                .w(44.0)
                .h(44.0)
                .rounded_full()
                .bg(Color::rgba(0.2, 0.4, 0.9, 1.0)),
        )
        // A translucent fill with a thick border, at a fractional size.
        .child(
            div()
                .class("t")
                .w(70.5)
                .h(40.3)
                .rounded(7.0)
                .bg(Color::rgba(0.9, 0.3, 0.2, 0.55))
                .border(3.0, Color::rgba(0.1, 0.1, 0.1, 0.8)),
        )
        // A sharp rect.
        .child(
            div()
                .class("t")
                .w(50.0)
                .h(30.0)
                .bg(Color::rgba(0.1, 0.6, 0.3, 1.0)),
        )
        // A rounded container that clips its child.
        .child(
            div()
                .class("t")
                .w(90.0)
                .h(46.0)
                .rounded(14.0)
                .overflow_clip()
                .bg(Color::rgba(0.9, 0.9, 0.95, 1.0))
                .child(
                    div()
                        .class("t")
                        .w(120.0)
                        .h(30.0)
                        .bg(Color::rgba(0.8, 0.2, 0.6, 1.0)),
                ),
        )
}

fn render(theme: ThemeBundle, mask: &str) -> Vec<u8> {
    let mut ui = Headless::with_scale(W, H, 2.0).unwrap();
    ui.theme(theme, ColorScheme::Light);
    ui.css(&format!(".t {{ {mask} }}"));
    ui.mount(|_| scene());
    ui.leave().settle();
    ui.snapshot().unwrap().data
}

fn differences(a: &[u8], b: &[u8]) -> (usize, u8) {
    let mut count = 0;
    let mut worst = 0;
    for (x, y) in a.chunks(4).zip(b.chunks(4)) {
        let d = x.iter().zip(y).map(|(p, q)| p.abs_diff(*q)).max().unwrap();
        if d > 0 {
            count += 1;
        }
        worst = worst.max(d);
    }
    (count, worst)
}

/// Corners large enough for the Expressive theme to give them a squircle curve.
fn squircle_scene() -> impl blinc_layout::prelude::ElementBuilder {
    div()
        .w(W as f32)
        .h(H as f32)
        .bg(Color::rgba(0.96, 0.97, 0.99, 1.0))
        .p(6.0)
        .child(
            div()
                .class("t")
                .w(240.0)
                .h(140.0)
                .rounded(36.0)
                .bg(Color::rgba(1.0, 1.0, 1.0, 1.0))
                .border(1.5, Color::rgba(0.2, 0.25, 0.35, 0.5)),
        )
}

fn render_squircle(mask: &str) -> Vec<u8> {
    let mut ui = Headless::with_scale(W, H, 2.0).unwrap();
    ui.theme(ExpressiveTheme::bundle(), ColorScheme::Light);
    ui.css(&format!(".t {{ {mask} }}"));
    ui.mount(|_| squircle_scene());
    ui.leave().settle();
    ui.snapshot().unwrap().data
}

#[test]
fn the_compact_shader_matches_the_core_shader() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let plain = render(HybridTheme::bundle(), "");
    // The control: a mask that does change pixels shows the CSS reaches the primitives.
    let dimmed = render(
        HybridTheme::bundle(),
        "mask-image: linear-gradient(to bottom, rgba(0,0,0,0.5), rgba(0,0,0,0.5));",
    );
    let (changed, _) = differences(&plain, &dimmed);
    assert!(
        changed > 1000,
        "the mask did not reach the elements ({changed} pixels differ)"
    );

    let core = render(
        HybridTheme::bundle(),
        "mask-image: linear-gradient(to bottom, #000, #000);",
    );
    let (count, worst) = differences(&plain, &core);
    assert!(
        worst <= 2,
        "{count} pixels differ from the core shader by up to {worst} of 255"
    );
}

#[test]
fn the_compact_shader_matches_the_core_shader_on_squircle_corners() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let plain = render_squircle("");
    let core = render_squircle("mask-image: linear-gradient(to bottom, #000, #000);");
    let (count, worst) = differences(&plain, &core);
    assert!(
        worst <= 2,
        "{count} pixels differ from the core shader by up to {worst} of 255"
    );
}
