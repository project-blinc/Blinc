#![cfg(feature = "headless")]
//! Closable tabs: the active trigger's close button shows, another's while it is hovered, and
//! pressing it reports the tab without selecting it, in a plain strip and a reorderable one.

use std::sync::{Arc, Mutex};

use blinc_cn::prelude::*;
use blinc_core::reactive::signal;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(480, 160).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

/// The element id ending in `suffix`.
fn id_ending(ui: &Headless, suffix: &str) -> String {
    ui.tree()
        .element_registry()
        .all_ids()
        .into_iter()
        .find(|id| id.ends_with(suffix))
        .unwrap_or_else(|| panic!("no element ending {suffix}"))
}

fn opacity(ui: &Headless, id: &str) -> f32 {
    let node = ui.find(&format!("#{id}")).unwrap();
    ui.tree().get_render_node(node).unwrap().props.opacity
}

fn check(reorderable: bool) {
    let Some(mut ui) = session() else { return };
    let closed = Arc::new(Mutex::new(Vec::<String>::new()));
    let reported = Arc::clone(&closed);
    let selected = signal("one".to_string());
    ui.mount(move |_| {
        let reported = Arc::clone(&reported);
        let tabs = tabs(selected)
            .tab("one", "One", || div())
            .tab("two", "Two", || div())
            .closable(move |value| reported.lock().unwrap().push(value.to_string()));
        let tabs = if reorderable {
            tabs.reorderable(|_| {})
        } else {
            tabs
        };
        div().w_full().h_full().child(tabs)
    });
    ui.settle();
    let close = id_ending(&ui, ":two:close");
    assert_eq!(
        opacity(&ui, &close),
        0.0,
        "an inactive tab's close button shows at rest"
    );
    assert_eq!(
        opacity(&ui, &id_ending(&ui, ":one:close")),
        1.0,
        "the active tab's close button is hidden"
    );

    // Hover the trigger's label, left of the button.
    let (cx, cy) = ui.centre(&format!("#{close}"));
    ui.move_to(cx - 30.0, cy);
    ui.advance(400).settle();
    assert_eq!(
        opacity(&ui, &close),
        1.0,
        "hovering the trigger did not show its close button"
    );

    ui.move_to(cx, cy).mouse_down().mouse_up();
    ui.advance(100).settle();
    assert_eq!(
        *closed.lock().unwrap(),
        ["two"],
        "the close button did not report its tab"
    );
    assert_eq!(
        selected.get(),
        "one",
        "pressing the close button selected its tab"
    );

    ui.move_to(cx - 30.0, cy).mouse_down().mouse_up();
    ui.advance(100).settle();
    assert_eq!(
        selected.get(),
        "two",
        "pressing the label no longer selects the tab"
    );
    assert_eq!(
        closed.lock().unwrap().len(),
        1,
        "pressing the label closed the tab"
    );
}

#[test]
fn a_closable_tab_closes_from_its_button() {
    check(false);
}

#[test]
fn a_closable_reorderable_tab_closes_from_its_button() {
    check(true);
}

#[test]
fn the_active_tabs_close_button_marks_its_hover() {
    let Some(mut ui) = session() else { return };
    let selected = signal("one".to_string());
    ui.mount(move |_| {
        div()
            .w_full()
            .h_full()
            .child(tabs(selected).tab("one", "One", || div()).closable(|_| {}))
    });
    ui.settle();
    let close = id_ending(&ui, ":one:close");
    let background = |ui: &Headless| {
        let node = ui.find(&format!("#{close}")).unwrap();
        format!(
            "{:?}",
            ui.tree().get_render_node(node).unwrap().props.background
        )
    };
    let rest = background(&ui);
    let (x, y) = ui.centre(&format!("#{close}"));
    ui.move_to(x, y);
    ui.advance(300).settle();
    assert_ne!(
        background(&ui),
        rest,
        "hovering the close button left it as it was"
    );
}

#[test]
fn the_close_buttons_hover_holds_however_the_pointer_arrives() {
    let Some(mut ui) = session() else { return };
    let selected = signal("one".to_string());
    ui.mount(move |_| {
        div().w_full().h_full().child(
            tabs(selected)
                .tab("one", "One", || div())
                .tab("two", "Two", || div())
                .closable(|_| {}),
        )
    });
    ui.settle();
    let hovered = |ui: &Headless, value: &str| {
        let close = id_ending(ui, &format!(":{value}:close"));
        let node = ui.find(&format!("#{close}")).unwrap();
        ui.tree()
            .get_render_node(node)
            .unwrap()
            .props
            .background
            .is_some()
    };
    let mut misses = Vec::new();
    for round in 0..3 {
        for value in ["one", "two"] {
            selected.set(value.to_string());
            ui.advance(300).settle();
            let close = id_ending(&ui, &format!(":{value}:close"));
            let (x, y) = ui.centre(&format!("#{close}"));
            // From outside the strip, from the label, and from the right of the trigger.
            for (label, from) in [
                ("below", (x, y + 60.0)),
                ("label", (x - 30.0, y)),
                ("right", (x + 14.0, y)),
            ] {
                ui.move_to(from.0, from.1);
                ui.advance(200).settle();
                ui.move_to(x, y);
                ui.advance(200).settle();
                if !hovered(&ui, value) {
                    misses.push(format!("round {round}, {value}, from {label}"));
                }
            }
        }
    }
    assert!(
        misses.is_empty(),
        "the close button's hover did not show: {misses:?}"
    );
}

/// The desktop runner skips hit testing while the pointer stays inside the last node it hit, so
/// the pill over the active trigger must not be that node, or the close button drawn over it
/// would not take the hover.
#[test]
fn the_active_pill_takes_no_pointer_input() {
    let Some(mut ui) = session() else { return };
    let selected = signal("one".to_string());
    ui.mount(move |_| {
        div()
            .w_full()
            .h_full()
            .child(tabs(selected).tab("one", "One", || div()).closable(|_| {}))
    });
    ui.settle();
    let pill = ui.find(".cn-tabs-indicator").expect("the active pill");
    assert!(
        ui.tree()
            .get_render_node(pill)
            .unwrap()
            .props
            .pointer_events_none,
        "the active pill takes the pointer"
    );
}
