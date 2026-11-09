#![cfg(feature = "headless")]
//! Reorderable tabs: dragging a trigger past its neighbour's middle swaps them in place, the
//! drop reports the new order, and a press that does not move still selects its tab.

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

/// A trigger's element id: the tabs give each `<key>:<value>`.
fn trigger_id(ui: &Headless, value: &str) -> String {
    let suffix = format!(":{value}");
    ui.tree()
        .element_registry()
        .all_ids()
        .into_iter()
        .find(|id| id.ends_with(&suffix))
        .expect("the trigger")
}

fn trigger_x(ui: &Headless, value: &str) -> f32 {
    let node = ui.find(&format!("#{}", trigger_id(ui, value))).unwrap();
    ui.tree().get_absolute_bounds(node).unwrap().x
}

#[test]
fn dragging_a_trigger_past_its_neighbour_reorders_the_tabs() {
    let Some(mut ui) = session() else { return };
    let order = Arc::new(Mutex::new(Vec::<String>::new()));
    let reported = Arc::clone(&order);
    let selected = signal("one".to_string());
    ui.mount(move |_| {
        let reported = Arc::clone(&reported);
        div().w_full().h_full().child(
            tabs(selected)
                .tab("one", "One", || div())
                .tab("two", "Two", || div())
                .tab("three", "Three", || div())
                .reorderable(move |next| *reported.lock().unwrap() = next),
        )
    });
    ui.settle();
    let (one, two) = (trigger_x(&ui, "one"), trigger_x(&ui, "two"));
    assert!(one < two);

    // Press "one" and drag it right, past the middle of "two".
    let (x, y) = ui.centre(&format!("#{}", trigger_id(&ui, "one")));
    ui.move_to(x, y).mouse_down();
    for step in 1..=12 {
        ui.move_to(x + step as f32 * 10.0, y);
        ui.frame(16);
    }
    ui.mouse_up();
    ui.advance(400).settle();

    assert_eq!(
        *order.lock().unwrap(),
        ["two", "one", "three"],
        "the drop did not report the new order"
    );
    assert!(
        trigger_x(&ui, "two") < trigger_x(&ui, "one"),
        "the triggers kept their places"
    );
    assert_eq!(selected.get(), "one", "the drag changed the selected tab");

    // A press that does not move selects.
    ui.click(&format!("#{}", trigger_id(&ui, "three")));
    ui.settle();
    assert_eq!(selected.get(), "three");
}

#[test]
fn dragging_a_trigger_left_past_its_neighbour_reorders_the_tabs_away_from_the_edge() {
    let Some(mut ui) = session() else { return };
    let order = Arc::new(Mutex::new(Vec::<String>::new()));
    let reported = Arc::clone(&order);
    let selected = signal("one".to_string());
    ui.mount(move |_| {
        let reported = Arc::clone(&reported);
        // Away from the window's left edge, so positions in the strip differ from the window's.
        div()
            .w_full()
            .h_full()
            .flex_row()
            .child(div().w(160.0))
            .child(
                div().flex_1().h_full().child(
                    tabs(selected)
                        .tab("one", "One", || div())
                        .tab("two", "Two", || div())
                        .tab("three", "Three", || div())
                        .reorderable(move |next| *reported.lock().unwrap() = next),
                ),
            )
    });
    ui.settle();

    // Press "three" and drag it left, past the middle of "two".
    let (x, y) = ui.centre(&format!("#{}", trigger_id(&ui, "three")));
    ui.move_to(x, y).mouse_down();
    for step in 1..=12 {
        ui.move_to(x - step as f32 * 10.0, y);
        ui.frame(16);
    }
    ui.mouse_up();
    ui.advance(400).settle();

    assert_eq!(
        *order.lock().unwrap(),
        ["one", "three", "two"],
        "the drop did not report the new order"
    );
    assert!(
        trigger_x(&ui, "three") < trigger_x(&ui, "two"),
        "the triggers kept their places"
    );
}

#[test]
fn a_trigger_dragged_right_can_be_dragged_back() {
    let Some(mut ui) = session() else { return };
    let order = Arc::new(Mutex::new(Vec::<String>::new()));
    let reported = Arc::clone(&order);
    let selected = signal("one".to_string());
    ui.mount(move |_| {
        let reported = Arc::clone(&reported);
        div().w_full().h_full().child(
            tabs(selected)
                .tab("one", "One", || div())
                .tab("two", "Two", || div())
                .closable(|_| {})
                .reorderable(move |next| *reported.lock().unwrap() = next),
        )
    });
    ui.settle();
    let drag = |ui: &mut Headless, value: &str, dx: f32| {
        let (x, y) = ui.centre(&format!("#{}", trigger_id(ui, value)));
        ui.move_to(x - 20.0, y).mouse_down();
        for step in 1..=12 {
            ui.move_to(x - 20.0 + dx * step as f32 / 12.0, y);
            ui.frame(16);
        }
        ui.mouse_up();
        ui.advance(400).settle();
    };
    drag(&mut ui, "one", 120.0);
    assert_eq!(
        *order.lock().unwrap(),
        ["two", "one"],
        "the first drag did not reorder"
    );
    drag(&mut ui, "one", -120.0);
    assert_eq!(
        *order.lock().unwrap(),
        ["one", "two"],
        "dragging back did not reorder"
    );
}

#[test]
fn the_tabs_reorder_while_the_drag_is_held() {
    let Some(mut ui) = session() else { return };
    let selected = signal("one".to_string());
    ui.mount(move |_| {
        div().w_full().h_full().child(
            tabs(selected)
                .tab("one", "One", || div())
                .tab("two", "Two", || div())
                .tab("three", "Three", || div())
                .tab("four", "Four", || div())
                .reorderable(|_| {}),
        )
    });
    ui.settle();
    let (x, y) = ui.centre(&format!("#{}", trigger_id(&ui, "one")));
    ui.move_to(x, y).mouse_down();
    let mut seen = Vec::new();
    for step in 1..=30 {
        ui.move_to(x + step as f32 * 10.0, y);
        ui.frame(16);
        let mut order: Vec<(f32, &str)> = ["one", "two", "three", "four"]
            .iter()
            .map(|v| (trigger_x(&ui, v), *v))
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        let at = order.iter().position(|(_, v)| *v == "one").unwrap();
        seen.push(at);
    }
    assert_eq!(
        seen.last(),
        Some(&3),
        "the held tab did not reach the end before the drop: {seen:?}"
    );
    ui.mouse_up();
}

/// Where trigger `value` is drawn: laid out, then moved by its transform.
fn drawn_x(ui: &Headless, value: &str) -> f32 {
    let node = ui.find(&format!("#{}", trigger_id(ui, value))).unwrap();
    let laid_out = ui.tree().get_absolute_bounds(node).unwrap().x;
    let shift = match &ui.tree().get_render_node(node).unwrap().props.transform {
        Some(blinc_core::Transform::Affine2D(a)) => a.elements[4],
        _ => 0.0,
    };
    laid_out + shift
}

#[test]
fn the_held_tab_follows_the_pointer_and_its_neighbour_slides_back_and_forth() {
    let Some(mut ui) = session() else { return };
    let selected = signal("one".to_string());
    ui.mount(move |_| {
        div().w_full().h_full().child(
            tabs(selected)
                .tab("one", "One", || div())
                .tab("two", "Two", || div())
                .tab("three", "Three", || div())
                .reorderable(|_| {}),
        )
    });
    ui.settle();
    let start = drawn_x(&ui, "one");
    let width = {
        let node = ui.find(&format!("#{}", trigger_id(&ui, "one"))).unwrap();
        ui.tree().get_absolute_bounds(node).unwrap().width
    };
    let (x, y) = ui.centre(&format!("#{}", trigger_id(&ui, "one")));
    ui.move_to(x, y).mouse_down();
    // Right past "two", back left past it, and right again, a few pixels a frame.
    let path: Vec<f32> = (1..=20)
        .chain((0..20).rev())
        .chain(1..=20)
        .map(|i| i as f32 * 4.0)
        .collect();
    let mut last_two = drawn_x(&ui, "two");
    let mut worst_jump: f32 = 0.0;
    for dx in path {
        ui.move_to(x + dx, y);
        ui.frame(16);
        let held = drawn_x(&ui, "one");
        assert!(
            (held - (start + dx)).abs() < 1.5,
            "the held tab is drawn at {held}, not under the pointer at {}",
            start + dx
        );
        let two = drawn_x(&ui, "two");
        worst_jump = worst_jump.max((two - last_two).abs());
        last_two = two;
    }
    ui.mouse_up();
    assert!(
        worst_jump < width / 2.0,
        "the neighbour jumped {worst_jump} in a frame instead of sliding"
    );
}
