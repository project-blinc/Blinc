#![cfg(feature = "headless")]
//! What nobody can see is not drawn: a scroll view skips rows outside its viewport, unless
//! something under a row reaches into it, and a closed collapsible draws none of its content.

use std::sync::{Arc, Mutex};

use blinc_cn::prelude::*;
use blinc_core::reactive::signal;
use blinc_core::{Color, RecordingContext, Size};
use blinc_layout::div::{Div, div};
use blinc_layout::prelude::*;
use blinc_layout::widgets::scroll::{Scroll, ScrollPhysics};
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const ROWS: usize = 200;
const ROW_H: f32 = 24.0;
/// The tall overlay on row 190, drawn far past the row.
const MAGENTA: [u8; 3] = [255, 0, 255];

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(400, 300).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

/// Row `i` is coloured by its index, so a pixel says which row is on screen.
fn row(i: usize) -> Div {
    div()
        .h(ROW_H)
        .w_full()
        .flex_shrink_0()
        .bg(Color::rgb(i as f32 / 255.0, 0.5, 0.2))
        .child(text(format!("Row {i}")).size(13.0).color(Color::WHITE))
}

fn rows() -> Div {
    (0..ROWS).fold(div().w_full().flex_col(), |d, i| d.child(row(i)))
}

/// Draw commands the settled tree paints.
fn draw_commands(ui: &Headless) -> usize {
    let mut rec = RecordingContext::new(Size::new(400.0, 300.0));
    ui.tree().render_snapshot(&mut rec);
    rec.commands().len()
}

fn near(a: u8, b: u8) -> bool {
    a.abs_diff(b) <= 3
}

#[test]
fn a_scroll_view_draws_the_rows_in_view_and_what_reaches_into_it() {
    let Some(mut ui) = session() else { return };
    let physics = Arc::new(Mutex::new(ScrollPhysics::default()));
    let held = Arc::clone(&physics);
    ui.mount(move |_| {
        let mut content = div().w_full().flex_col();
        for i in 0..ROWS {
            let mut r = row(i).relative();
            if i == 190 {
                // Far taller than its row: it reaches up past where row 100 is. Painted after
                // the rows it covers, so nothing paints over it.
                r = r.child(
                    div()
                        .absolute()
                        .top(-2200.0)
                        .left(320.0)
                        .w(60.0)
                        .h(2400.0)
                        .bg(Color::rgb(1.0, 0.0, 1.0)),
                );
            }
            content = content.child(r);
        }
        div().w(400.0).h(300.0).child(
            Scroll::with_physics(Arc::clone(&held))
                .w(400.0)
                .h(300.0)
                .child(content),
        )
    });
    ui.settle();
    let all = ROWS * 3;
    let drawn = draw_commands(&ui);
    assert!(
        drawn < all / 3,
        "the scroll view drew {drawn} commands for 12 visible rows"
    );

    // Scroll so row 100 is at the top; row 190 is far below the viewport.
    physics.lock().unwrap().offset_y = -(100.0 * ROW_H);
    ui.frame(16);
    ui.settle();
    let frame = ui.snapshot().unwrap();
    let px = frame.get_pixel(100, (ROW_H / 2.0) as u32).unwrap();
    assert!(near(px[0], 100), "row 100 is not drawn at the top: {px:?}");
    let has_text = (0..60)
        .any(|x| (0..ROW_H as u32).any(|y| frame.get_pixel(x, y).is_some_and(|px| px[1] > 220)));
    assert!(has_text, "row 100's text is not drawn");
    let reach = frame.get_pixel(350, 150).unwrap();
    assert_eq!(
        &reach[..3],
        &MAGENTA,
        "the overlay reaching from row 190 is not drawn"
    );
}

#[test]
fn a_closed_collapsible_draws_none_of_its_content() {
    let Some(mut ui) = session() else { return };
    let open = signal(false);
    ui.mount(move |_| {
        div()
            .w(400.0)
            .h(300.0)
            .child(collapsible(open).content(rows))
    });
    ui.settle();
    let closed = draw_commands(&ui);
    assert!(closed < 60, "the closed collapsible drew {closed} commands");

    open.set(true);
    ui.advance(600).settle();
    assert!(
        draw_commands(&ui) > ROWS * 2,
        "the open collapsible did not draw its rows"
    );
    let px = ui
        .snapshot()
        .unwrap()
        .get_pixel(100, (ROW_H / 2.0) as u32)
        .unwrap();
    assert!(
        near(px[0], 0) && near(px[1], 128),
        "row 0 is not drawn when open: {px:?}"
    );
}
