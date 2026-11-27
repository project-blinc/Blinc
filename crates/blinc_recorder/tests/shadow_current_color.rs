#![cfg(feature = "headless")]
//! A shadow that names no colour takes the text colour of its element, as CSS `currentColor`:
//! inherited, following a hover, and a colour that transitions run from.

use blinc_core::Color;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{BlincTheme, ColorScheme};

const CSS: &str = "
    .box { position: absolute; width: 60px; height: 40px; background: #ffffff; }
    #red { color: #ff0000; }
    #inherits { left: 20px; top: 20px; box-shadow: 0 0 8px; }
    #inset { left: 140px; top: 20px; color: #00ff00; box-shadow: inset 0 0 8px currentColor; }
    #inset:hover { color: #0000ff; }
    #glow { left: 20px; top: 100px; color: #ff0000; box-shadow: 0 0 8px;
            transition: box-shadow 200ms linear; }
    #glow:hover { box-shadow: 0 0 8px #0000ff; }
    #shadowed { color: #ff00ff; }
    #label { position: absolute; left: 140px; top: 80px; font-size: 32px; text-shadow: 0 44px; }
    #drop { left: 20px; top: 170px; color: #ff0000; filter: drop-shadow(14px 0 0); }
";

#[test]
fn a_shadow_without_a_colour_takes_the_text_colour() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(260, 240).unwrap();
    ui.theme(BlincTheme::bundle().with_css(CSS), ColorScheme::Light);
    ui.mount(|_| {
        div()
            .w(260.0)
            .h(240.0)
            .bg(Color::WHITE)
            .child(div().id("red").child(div().id("inherits").class("box")))
            .child(div().id("inset").class("box"))
            .child(div().id("glow").class("box"))
            .child(
                div()
                    .id("shadowed")
                    .child(text("HHHH").no_wrap().id("label")),
            )
            .child(div().id("drop").class("box"))
    });
    ui.leave().settle();

    let pixel = |ui: &mut Headless, x: u32, y: u32| {
        let p = ui.snapshot().unwrap().get_pixel(x, y).unwrap();
        [i32::from(p[0]), i32::from(p[1]), i32::from(p[2])]
    };
    // Which channels stand out from the others.
    let tint = |p: [i32; 3]| {
        let low = p[0].min(p[1]).min(p[2]);
        [p[0] - low > 24, p[1] - low > 24, p[2] - low > 24]
    };

    // Just outside the left edge: the colour the parent sets.
    let outer = pixel(&mut ui, 17, 40);
    assert_eq!(tint(outer), [true, false, false], "inherited: {outer:?}");

    // Just inside the left edge: the element's own colour, then its hover colour.
    let inset = pixel(&mut ui, 142, 40);
    assert_eq!(tint(inset), [false, true, false], "own: {inset:?}");
    ui.hover("#inset").settle();
    let hovered = pixel(&mut ui, 142, 40);
    assert_eq!(tint(hovered), [false, false, true], "hovered: {hovered:?}");

    // The transition starts from the text colour and passes through the colours between.
    assert_eq!(tint(pixel(&mut ui, 17, 120)), [true, false, false]);
    ui.hover("#glow").advance(100);
    let midway = pixel(&mut ui, 17, 120);
    assert_eq!(tint(midway), [true, false, true], "midway: {midway:?}");
    ui.settle();
    assert_eq!(tint(pixel(&mut ui, 17, 120)), [false, false, true]);

    // A text shadow in the inherited colour, offset below its text.
    let b = ui
        .tree()
        .get_absolute_bounds(ui.find("#label").unwrap())
        .unwrap();
    let frame = ui.snapshot().unwrap();
    let below = (b.y + b.height) as u32 + 6;
    // Text that does not wrap is as wide as its glyphs, past a box of no width.
    let magenta = (b.x as u32..b.x as u32 + 90)
        .flat_map(|x| (below..below + 32).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let p = frame.get_pixel(x, y).unwrap();
            p[0] > 180 && p[2] > 180 && p[1] < 100
        })
        .count();
    assert!(
        magenta > 20,
        "the text shadow drew {magenta} pixels of its colour"
    );

    // A drop shadow filter past the right edge.
    let drop = pixel(&mut ui, 88, 190);
    assert_eq!(tint(drop), [true, false, false], "drop-shadow: {drop:?}");
}
