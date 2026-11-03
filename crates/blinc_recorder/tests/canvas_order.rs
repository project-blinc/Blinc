#![cfg(feature = "headless")]
//! A canvas paints in tree order: what comes after it in the tree is drawn over it, whether it is a
//! rectangle or a path.

use blinc_core::{Color, Path, Rect};
use blinc_layout::canvas::canvas;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 160;
const H: u32 = 120;

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(W, H).ok()?;
    ui.theme(HybridTheme::bundle(), ColorScheme::Light);
    Some(ui)
}

fn red_rect() -> blinc_layout::canvas::Canvas {
    canvas(|ctx, bounds| {
        use blinc_core::DrawContext;
        ctx.fill_rect(
            Rect::new(0.0, 0.0, bounds.width, bounds.height),
            0.0.into(),
            Color::RED.into(),
        );
    })
    .absolute()
    .left(20.0)
    .top(20.0)
    .w(60.0)
    .h(60.0)
}

fn red_path() -> blinc_layout::canvas::Canvas {
    canvas(|ctx, bounds| {
        use blinc_core::DrawContext;
        let path = Path::new()
            .move_to(0.0, 0.0)
            .line_to(bounds.width, 0.0)
            .line_to(bounds.width, bounds.height)
            .line_to(0.0, bounds.height)
            .close();
        ctx.fill_path(&path, Color::RED.into());
    })
    .absolute()
    .left(20.0)
    .top(20.0)
    .w(60.0)
    .h(60.0)
}

fn green_box() -> blinc_layout::div::Div {
    div()
        .absolute()
        .left(50.0)
        .top(50.0)
        .w(60.0)
        .h(60.0)
        .bg(Color::GREEN)
}

fn at(ui: &mut Headless, x: u32, y: u32) -> [u8; 3] {
    let p = ui.snapshot().unwrap().get_pixel(x, y).unwrap();
    [p[0], p[1], p[2]]
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 4)
}

fn check(make: fn() -> blinc_layout::canvas::Canvas, canvas_first: bool) {
    let Some(mut ui) = session() else { return };
    ui.mount(move |_| {
        let host = div().w(W as f32).h(H as f32).bg(Color::BLUE);
        if canvas_first {
            host.child(make()).child(green_box())
        } else {
            host.child(green_box()).child(make())
        }
    });
    ui.advance(50);
    // The overlap of the two boxes: whichever is later in the tree is on top.
    let (top, other) = if canvas_first {
        ([0, 255, 0], [255, 0, 0])
    } else {
        ([255, 0, 0], [0, 255, 0])
    };
    let green = Color::GREEN;
    let _ = green;
    let overlap = at(&mut ui, 65, 65);
    assert!(
        near(overlap, top),
        "overlap is {overlap:?}, expected {top:?}"
    );
    let canvas_only = at(&mut ui, 30, 30);
    assert!(
        near(canvas_only, [255, 0, 0]),
        "canvas only: {canvas_only:?}"
    );
    let box_only = at(&mut ui, 100, 100);
    assert!(near(box_only, [0, 255, 0]), "box only: {box_only:?}");
    let _ = other;
}

#[test]
fn a_later_background_sibling_covers_a_rectangle_canvas() {
    check(red_rect, true);
}

#[test]
fn a_later_background_sibling_covers_a_path_canvas() {
    check(red_path, true);
}

#[test]
fn a_canvas_after_a_background_sibling_covers_it() {
    check(red_rect, false);
}
