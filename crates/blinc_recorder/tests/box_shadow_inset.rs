#![cfg(feature = "headless")]
//! An inset `box-shadow` paints inside the element in the window's paint walk, and a later
//! `box-shadow` without an inset layer replaces it.

use blinc_core::Color;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{BlincTheme, ColorScheme};

// The theme stylesheet is process-wide, and concurrent sessions in one process can
// lose it, so both cases share one sheet and one test.
const CSS: &str = "
    #plain {
        width: 120px;
        height: 50px;
        // background-color: #ffffff;
        box-shadow: 0px 0px 12px #ff0000 inset;
        border: 1px solid #000000;
    }
    #hovered {
        width: 120px;
        height: 50px;
        background: #ffffff;
        box-shadow: 0px 0px 12px #ff0000 inset;
    }
    #hovered:hover { box-shadow: 0 4px 8px rgba(0, 0, 0, 0.3); }
";

const W: u32 = 200;
const H: u32 = 160;
// Two 120x50 cards stacked 20px apart and centred: x 40..160, y 20..70 and 90..140.
const PLAIN_EDGE: (u32, u32) = (156, 45);
const PLAIN_CENTRE: (u32, u32) = (100, 45);
const HOVERED_EDGE: (u32, u32) = (156, 115);

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(W, H).ok()?;
    ui.theme(BlincTheme::bundle().with_css(CSS), ColorScheme::Light);
    ui.mount(|_| {
        div()
            .w_full()
            .h_full()
            .bg(Color::WHITE)
            .flex_col()
            .items_center()
            .justify_center()
            .gap(5.0)
            .child(div().id("plain"))
            .child(div().id("hovered"))
    });
    ui.settle();
    Some(ui)
}

fn pixel(ui: &mut Headless, (x, y): (u32, u32)) -> [u8; 4] {
    ui.snapshot().unwrap().get_pixel(x, y).unwrap()
}

fn is_red(p: [u8; 4]) -> bool {
    p[0] > 200 && p[1] < 160 && p[2] < 160
}

#[test]
fn inset_box_shadows_paint_and_an_outer_only_value_replaces_them() {
    let Some(mut ui) = session() else {
        return;
    };

    // No background, and a `//` line the parser skips.
    let edge = pixel(&mut ui, PLAIN_EDGE);
    assert!(is_red(edge), "inside the right edge: {edge:?}");
    let centre = pixel(&mut ui, PLAIN_CENTRE);
    assert_eq!(
        centre,
        [255, 255, 255, 255],
        "the shadow fades out by the centre"
    );

    assert!(is_red(pixel(&mut ui, HOVERED_EDGE)), "inset before hover");
    ui.hover("#hovered").settle();
    let edge = pixel(&mut ui, HOVERED_EDGE);
    assert!(
        !is_red(edge),
        "hover's outer-only value clears the inset: {edge:?}"
    );
}
