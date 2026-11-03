#![cfg(feature = "headless")]
//! Tab traversal, Enter/Space activation and the focus ring.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use blinc_cn::prelude::*;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const TAB: u32 = 9;
const ENTER: u32 = 13;
const SPACE: u32 = 32;

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

fn counter() -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
    (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)))
}

fn mount_two_buttons(ui: &mut Headless, a: &Arc<AtomicUsize>, b: &Arc<AtomicUsize>) {
    let (a, b) = (a.clone(), b.clone());
    ui.mount(move |_| {
        let (a, b) = (a.clone(), b.clone());
        div()
            .flex_row()
            .gap(2.0)
            .p(4.0)
            .child(button("First").on_click(move |_| {
                a.fetch_add(1, Ordering::SeqCst);
            }))
            .child(button("Second").disabled(true))
            .child(button("Third").on_click(move |_| {
                b.fetch_add(1, Ordering::SeqCst);
            }))
    });
    ui.settle();
}

fn focused_index(ui: &Headless) -> Option<usize> {
    let focused = ui.focused()?;
    ui.tree().tab_stops().iter().position(|n| *n == focused)
}

#[test]
fn tab_walks_the_enabled_controls_in_order_and_wraps() {
    let Some(mut ui) = session() else { return };
    let (a, b) = counter();
    mount_two_buttons(&mut ui, &a, &b);
    assert_eq!(
        ui.tree().tab_stops().len(),
        2,
        "the disabled button is skipped"
    );
    assert_eq!(focused_index(&ui), None);
    ui.key(TAB);
    assert_eq!(focused_index(&ui), Some(0));
    ui.key(TAB);
    assert_eq!(focused_index(&ui), Some(1));
    ui.key(TAB);
    assert_eq!(focused_index(&ui), Some(0));
}

#[test]
fn shift_tab_walks_back() {
    let Some(mut ui) = session() else { return };
    let (a, b) = counter();
    mount_two_buttons(&mut ui, &a, &b);
    ui.key_with_shift(TAB, true);
    assert_eq!(focused_index(&ui), Some(1));
    ui.key_with_shift(TAB, true);
    assert_eq!(focused_index(&ui), Some(0));
}

#[test]
fn enter_and_space_click_the_focused_control() {
    let Some(mut ui) = session() else { return };
    let (a, b) = counter();
    mount_two_buttons(&mut ui, &a, &b);
    ui.key(ENTER);
    assert_eq!(a.load(Ordering::SeqCst), 0, "nothing is focused yet");
    ui.key(TAB);
    ui.key(ENTER);
    assert_eq!((a.load(Ordering::SeqCst), b.load(Ordering::SeqCst)), (1, 0));
    ui.key(TAB);
    ui.key(SPACE);
    assert_eq!((a.load(Ordering::SeqCst), b.load(Ordering::SeqCst)), (1, 1));
}

#[test]
fn a_press_takes_keyboard_focus_away() {
    let Some(mut ui) = session() else { return };
    let (a, b) = counter();
    mount_two_buttons(&mut ui, &a, &b);
    ui.key(TAB);
    assert_eq!(focused_index(&ui), Some(0));
    ui.move_to(390.0, 190.0);
    ui.mouse_down();
    ui.mouse_up();
    ui.settle();
    assert_eq!(focused_index(&ui), None);
    ui.key(ENTER);
    assert_eq!(a.load(Ordering::SeqCst), 0);
}

#[test]
fn a_focused_control_shows_the_ring() {
    let Some(mut ui) = session() else { return };
    let (a, b) = counter();
    mount_two_buttons(&mut ui, &a, &b);
    let before = ui.snapshot().expect("snapshot");
    ui.key(TAB);
    ui.settle();
    let after = ui.snapshot().expect("snapshot");
    assert_ne!(before.data, after.data, "focus draws an outline");
}

#[test]
fn tab_scrolls_the_next_control_into_view() {
    use blinc_layout::widgets::scroll::scroll;
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        scroll().w_full().h(80.0).child(
            div()
                .flex_col()
                .gap(2.0)
                .children((0..12).map(|i| button(format!("Item {i}")))),
        )
    });
    ui.settle();
    let scroller = ui
        .find(".blinc-scroll")
        .or_else(|| ui.tree().root())
        .expect("scroller");
    let view = ui.tree().get_absolute_bounds(scroller).expect("bounds");
    for _ in 0..12 {
        ui.key(TAB);
        let node = ui.focused().expect("focused");
        let b = ui.tree().get_absolute_bounds(node).expect("bounds");
        assert!(
            b.y >= view.y - 0.5 && b.y + b.height <= view.y + view.height + 0.5,
            "focused control at {b:?} is outside the view {view:?}"
        );
    }
    ui.key_with_shift(TAB, true);
    ui.key_with_shift(TAB, true);
    let node = ui.focused().expect("focused");
    let b = ui.tree().get_absolute_bounds(node).expect("bounds");
    assert!(b.y >= view.y - 0.5 && b.y + b.height <= view.y + view.height + 0.5);
}

#[test]
fn tab_enters_a_text_input_and_leaves_it() {
    use blinc_layout::widgets::text_input::{focused_text_input_node_id, text_input_data};
    let Some(mut ui) = session() else { return };
    let data = text_input_data();
    let (a, b) = counter();
    let (a2, b2) = (a.clone(), b.clone());
    ui.mount(move |_| {
        let (a, b) = (a2.clone(), b2.clone());
        div()
            .flex_row()
            .gap(2.0)
            .p(4.0)
            .child(button("Before").on_click(move |_| {
                a.fetch_add(1, Ordering::SeqCst);
            }))
            .child(input(&data).w(120.0))
            .child(button("After").on_click(move |_| {
                b.fetch_add(1, Ordering::SeqCst);
            }))
    });
    ui.settle();
    assert_eq!(ui.tree().tab_stops().len(), 3);
    ui.key(TAB);
    ui.key(TAB);
    assert!(
        focused_text_input_node_id().is_some(),
        "the input is focused"
    );
    ui.key(TAB);
    assert!(focused_text_input_node_id().is_none(), "Tab left the input");
    ui.key(ENTER);
    assert_eq!((a.load(Ordering::SeqCst), b.load(Ordering::SeqCst)), (0, 1));
}

#[test]
fn the_interactive_cn_controls_are_tab_stops() {
    let Some(mut ui) = session() else { return };
    ui.mount(|ctx| {
        let on = ctx.use_signal(false);
        div()
            .flex_col()
            .gap(2.0)
            .p(4.0)
            .child(checkbox(on))
            .child(switch(on))
            .child(blinc_cn::components::toggle::toggle(on).label("T"))
            .child(accordion().item("one", "One", || div()))
            .child(collapsible_section("Section", on, || div()))
    });
    ui.settle();
    let stops = ui.tree().tab_stops().len();
    assert!(stops >= 5, "got {stops} stops");
}

#[test]
fn arrows_move_a_slider_focused_by_tab() {
    use blinc_core::reactive::signal;
    let Some(mut ui) = session() else { return };
    let value = signal(50.0f32);
    ui.mount(move |_| {
        div()
            .p(4.0)
            .child(
                blinc_cn::components::slider::slider(value)
                    .min(0.0)
                    .max(100.0)
                    .step(5.0),
            )
            .child(button("Next"))
    });
    ui.settle();
    ui.key(39);
    assert_eq!(value.get(), 50.0, "an unfocused slider ignores keys");
    ui.key(TAB);
    ui.key(39);
    assert_eq!(value.get(), 55.0);
    ui.key(37);
    ui.key(37);
    assert_eq!(value.get(), 45.0);
    ui.key(TAB);
    ui.key(39);
    assert_eq!(value.get(), 45.0, "Tab moved focus away");
}

#[test]
fn tab_stays_inside_an_open_dialog() {
    let Some(mut ui) = session() else { return };
    let (a, b) = counter();
    mount_two_buttons(&mut ui, &a, &b);
    let outside = ui.tree().tab_stops();
    blinc_cn::dialog()
        .title("Sure?")
        .confirm_text("Yes")
        .cancel_text("No")
        .show();
    ui.settle();
    let stops = ui.tree().tab_stops();
    assert!(
        stops.iter().all(|s| !outside.contains(s)),
        "only the dialog's stops remain"
    );
    assert!(stops.len() >= 2);
    for _ in 0..(stops.len() * 2) {
        ui.key(TAB);
        let focused = ui.focused().expect("focused");
        assert!(stops.contains(&focused), "Tab left the dialog");
    }
}
