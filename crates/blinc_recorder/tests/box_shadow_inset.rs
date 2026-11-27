#![cfg(feature = "headless")]
//! Box shadows paint as a browser paints them: an inset shadow inside the padding box, a
//! coloured shadow in its own colour, and a later `box-shadow` without an inset layer replaces
//! the inset one. The expected pixels come from the CSS model, computed here: the colour times
//! the coverage of a shape blurred by a Gaussian whose deviation is half the blur radius.

use blinc_core::Color;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{BlincTheme, ColorScheme};

// The theme stylesheet is process-wide, and concurrent sessions in one process can
// lose it, so every case shares one sheet and one test.
const CSS: &str = "
    .card { position: absolute; width: 120px; height: 50px; }
    #plain { left: 20px; top: 20px; box-shadow: 0px 0px 20px #b8bcb5 inset; }
    #offset {
        left: 180px; top: 20px; border: 4px solid #000000;
        box-shadow: inset 12px -6px 8px 2px rgba(0, 0, 255, 0.6);
    }
    /* A radius near half the height, which the theme leaves circular. */
    #round {
        left: 20px; top: 100px; border-radius: 24px;
        box-shadow: inset 0 0 10px 4px #ff0000;
    }
    #hard { left: 180px; top: 100px; box-shadow: inset 10px 0 0 0 #ff0000; }
    #outer { left: 20px; top: 190px; background: #ffffff; box-shadow: 0 0 20px #b8bcb5; }
    #hovered {
        left: 180px; top: 190px; background: #ffffff;
        box-shadow: 0px 0px 12px #ff0000 inset;
    }
    #hovered:hover { box-shadow: 0 4px 8px rgba(0, 0, 0, 0.3); }
";

const W: u32 = 340;
const H: u32 = 280;

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(W, H).ok()?;
    ui.theme(BlincTheme::bundle().with_css(CSS), ColorScheme::Light);
    ui.mount(|_| {
        let card = |id: &str| div().id(id).class("card");
        div()
            .w(W as f32)
            .h(H as f32)
            .bg(Color::WHITE)
            .child(card("plain"))
            .child(card("offset"))
            .child(card("round"))
            .child(card("hard"))
            .child(card("outer"))
            .child(card("hovered"))
    });
    ui.leave().settle();
    Some(ui)
}

/// A rect `(left, top, right, bottom)` with one corner radius.
#[derive(Clone, Copy)]
struct Shape {
    rect: [f64; 4],
    radius: f64,
}

/// The error function, to about seven places.
fn erf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.5 * x.abs());
    let poly = [
        1.00002368,
        0.37409196,
        0.09678418,
        -0.18628806,
        0.27886807,
        -1.13520398,
        1.48851587,
        -0.82215223,
        0.17087277,
    ]
    .iter()
    .rev()
    .fold(0.0, |sum, c| sum * t + c);
    let tail = t * (-x * x - 1.26551223 + t * poly).exp();
    (1.0 - tail).copysign(x)
}

/// How much of a Gaussian centred on `p` the shape covers, summed over thin rows.
fn coverage(shape: Shape, (px, py): (f64, f64), sigma: f64) -> f64 {
    let [x0, y0, x1, y1] = shape.rect;
    if sigma <= 0.0 {
        return f64::from(px > x0 && px < x1 && py > y0 && py < y1);
    }
    let below = |t: f64, at: f64| 0.5 * (1.0 + erf((t - at) / (sigma * std::f64::consts::SQRT_2)));
    let rows = 4000;
    let step = (y1 - y0) / rows as f64;
    (0..rows)
        .map(|i| {
            let row = y0 + step * (i as f64 + 0.5);
            let into = (y0 + shape.radius - row)
                .max(row - (y1 - shape.radius))
                .max(0.0);
            let inset = shape.radius - (shape.radius * shape.radius - into * into).max(0.0).sqrt();
            let across = below(x1 - inset, px) - below(x0 + inset, px);
            across * (below(row + step / 2.0, py) - below(row - step / 2.0, py))
        })
        .sum()
}

fn over(bg: [f64; 3], color: [f64; 3], alpha: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| bg[i] + (color[i] - bg[i]) * alpha)
}

/// Checks the pixels of `region` on a grid against `want`, at each pixel's centre.
fn matches(
    ui: &mut Headless,
    what: &str,
    region: [u32; 4],
    want: impl Fn((f64, f64)) -> Option<[f64; 3]>,
) {
    let frame = ui.snapshot().unwrap();
    let mut checked = 0;
    for y in (region[1]..region[3]).step_by(3) {
        for x in (region[0]..region[2]).step_by(3) {
            let at = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let Some(want) = want(at) else { continue };
            let got = frame.get_pixel(x, y).unwrap();
            let off = (0..3)
                .map(|i| (f64::from(got[i]) - want[i]).abs())
                .fold(0.0, f64::max);
            assert!(off <= 3.0, "{what} at {x},{y}: {got:?}, CSS gives {want:?}");
            checked += 1;
        }
    }
    assert!(checked > 20, "{what}: only {checked} pixels");
}

const WHITE: [f64; 3] = [255.0, 255.0, 255.0];
const GREY_GREEN: [f64; 3] = [184.0, 188.0, 181.0];

#[test]
fn box_shadows_paint_as_css_describes() {
    let Some(mut ui) = session() else {
        return;
    };
    let rect = |left: f64, top: f64, right: f64, bottom: f64| Shape {
        rect: [left, top, right, bottom],
        radius: 0.0,
    };
    // Inside the padding box, an inset shadow is what its blurred hole does not cover.
    let inset = |pad: Shape, hole: Shape, sigma: f64, color: [f64; 3], alpha: f64| {
        move |at: (f64, f64)| {
            // The antialiased edge of a shape is not compared.
            let [x0, y0, x1, y1] = pad.rect;
            let inner = Shape {
                rect: [x0 + 1.0, y0 + 1.0, x1 - 1.0, y1 - 1.0],
                radius: (pad.radius - 1.0).max(0.0),
            };
            (coverage(inner, at, 0.0) > 0.0 && in_corners(inner, at))
                .then(|| over(WHITE, color, alpha * (1.0 - coverage(hole, at, sigma))))
        }
    };

    let pad = rect(20.0, 20.0, 140.0, 70.0);
    matches(
        &mut ui,
        "a blurred inset shadow",
        [20, 20, 140, 70],
        inset(pad, pad, 10.0, GREY_GREEN, 1.0),
    );

    // A 4px border: the padding box starts inside it, and the hole is that box moved and shrunk.
    let pad = rect(184.0, 24.0, 296.0, 66.0);
    let hole = rect(
        184.0 + 12.0 + 2.0,
        24.0 - 6.0 + 2.0,
        296.0 + 12.0 - 2.0,
        66.0 - 6.0 - 2.0,
    );
    matches(
        &mut ui,
        "an offset inset shadow",
        [184, 24, 296, 66],
        inset(pad, hole, 4.0, [0.0, 0.0, 255.0], 0.6),
    );

    let pad = Shape {
        rect: [20.0, 100.0, 140.0, 150.0],
        radius: 24.0,
    };
    let hole = Shape {
        rect: [24.0, 104.0, 136.0, 146.0],
        radius: 20.0,
    };
    matches(
        &mut ui,
        "a rounded inset shadow",
        [20, 100, 140, 150],
        inset(pad, hole, 5.0, [255.0, 0.0, 0.0], 1.0),
    );

    let pad = rect(180.0, 100.0, 300.0, 150.0);
    let hole = rect(190.0, 100.0, 310.0, 150.0);
    matches(
        &mut ui,
        "an unblurred inset shadow",
        [180, 100, 300, 150],
        inset(pad, hole, 0.0, [255.0, 0.0, 0.0], 1.0),
    );

    // Outside the box, beside the middle of each side, where the corners do not reach.
    let outer = rect(20.0, 190.0, 140.0, 240.0);
    for region in [[2, 205, 19, 226], [60, 172, 100, 189], [60, 241, 100, 258]] {
        matches(&mut ui, "a coloured outer shadow", region, |at| {
            Some(over(WHITE, GREY_GREEN, coverage(outer, at, 10.0)))
        });
    }

    // Hovering replaces the inset shadow by the rule's outer one.
    let edge = |ui: &mut Headless| ui.snapshot().unwrap().get_pixel(297, 215).unwrap();
    let inset = edge(&mut ui);
    assert!(
        inset[0] > 200 && inset[1] < 200,
        "inset before hover: {inset:?}"
    );
    ui.hover("#hovered").settle();
    let hovered = edge(&mut ui);
    assert_eq!(
        hovered,
        [255, 255, 255, 255],
        "the hover value clears the inset"
    );
}

/// Whether a point is inside the rounded corners of a shape.
fn in_corners(shape: Shape, (x, y): (f64, f64)) -> bool {
    let [x0, y0, x1, y1] = shape.rect;
    let r = shape.radius;
    let dx = (x0 + r - x).max(x - (x1 - r)).max(0.0);
    let dy = (y0 + r - y).max(y - (y1 - r)).max(0.0);
    dx * dx + dy * dy <= r * r
}
