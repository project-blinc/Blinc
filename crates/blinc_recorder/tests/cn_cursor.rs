#![cfg(feature = "headless")]
//! The cursor a `cn` control shows under the pointer.

use blinc_cn::prelude::*;
use blinc_layout::div::div;
use blinc_layout::element::CursorStyle;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(400, 200).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

#[test]
fn a_button_shows_the_pointer_cursor() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| div().w_full().h_full().p(4.0).child(button("Save")));
    ui.settle();
    let (x, y) = ui.centre(".cn-button");
    ui.move_to(x, y);
    assert_eq!(ui.cursor_at(x, y), Some(CursorStyle::Pointer));
}

#[test]
fn a_horizontal_divider_shows_the_east_west_cursor() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        resizable_group()
            .direction(ResizeDirection::Horizontal)
            .panel(resizable_panel().default_size(150.0).child(div()))
            .panel(resizable_panel().flex_grow().child(div()))
    });
    ui.settle();
    let (x, y) = ui.centre(".cn-resizable-handle");
    ui.move_to(x, y);
    assert_eq!(ui.cursor_at(x, y), Some(CursorStyle::ResizeEW));
}

#[test]
fn a_vertical_divider_shows_the_north_south_cursor() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        resizable_group()
            .direction(ResizeDirection::Vertical)
            .panel(resizable_panel().default_size(80.0).child(div()))
            .panel(resizable_panel().flex_grow().child(div()))
    });
    ui.settle();
    let (x, y) = ui.centre(".cn-resizable-handle");
    ui.move_to(x, y);
    assert_eq!(ui.cursor_at(x, y), Some(CursorStyle::ResizeNS));
}

#[test]
fn a_divider_keeps_its_cursor_across_its_hit_area() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        resizable_group()
            .direction(ResizeDirection::Horizontal)
            .panel(resizable_panel().default_size(150.0).child(div()))
            .panel(resizable_panel().flex_grow().child(div()))
    });
    ui.settle();
    let handle = ui.find(".cn-resizable-handle").expect("handle");
    let b = ui.tree().get_absolute_bounds(handle).expect("bounds");
    for dx in [0.5, b.width / 2.0, b.width - 0.5] {
        for dy in [1.0, b.height / 2.0, b.height - 1.0] {
            assert_eq!(
                ui.cursor_at(b.x + dx, b.y + dy),
                Some(CursorStyle::ResizeEW),
                "at +{dx},+{dy} of {b:?}"
            );
        }
    }
}

#[test]
fn a_disabled_button_shows_not_allowed() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w_full()
            .h_full()
            .p(4.0)
            .child(button("Save").disabled(true))
    });
    ui.settle();
    assert_eq!(ui.cursor_over(".cn-button"), Some(CursorStyle::NotAllowed));
}
