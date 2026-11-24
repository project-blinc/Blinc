#![cfg(feature = "headless")]
//! Calendar popover feedback, bindings and dismissal.

use blinc_cn::{CalendarDay, date_picker};
use blinc_core::reactive::{Signal, signal};
use blinc_layout::{LayoutNodeId, div::div, renderer::ElementType};
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};
use std::sync::{Arc, Mutex};

const TODAY: CalendarDay = CalendarDay {
    year: 2026,
    month: 11,
    day: 3,
};

struct Scene {
    ui: Headless,
    date: Signal<Option<CalendarDay>>,
    disabled: Signal<bool>,
    placeholder: Signal<String>,
    changes: Arc<Mutex<Vec<Option<CalendarDay>>>>,
}

fn scene(scheme: ColorScheme, initial: Option<CalendarDay>) -> Scene {
    let mut ui = Headless::new(640, 580).expect("offscreen GPU adapter");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        scheme,
    );
    let date = signal(initial);
    let disabled = signal(false);
    let placeholder = signal("Choose a date".to_owned());
    let changes = Arc::new(Mutex::new(Vec::new()));
    let seen = changes.clone();
    ui.mount(move |_| {
        let seen = seen.clone();
        div()
            .p(8.0)
            .flex_col()
            .gap(4.0)
            .child(
                date_picker(date)
                    .today(TODAY)
                    .placeholder(placeholder)
                    .disabled(disabled)
                    .on_change(move |date| seen.lock().unwrap().push(date)),
            )
            .child(blinc_cn::button("After"))
    })
    .settle();
    Scene {
        ui,
        date,
        disabled,
        placeholder,
        changes,
    }
}

fn text_at(ui: &Headless, root: LayoutNodeId) -> String {
    let mut pending = vec![root];
    let mut result = String::new();
    while let Some(node) = pending.pop() {
        if let Some(render) = ui.tree().get_render_node(node) {
            if let ElementType::Text(t) = &render.element_type {
                result.push_str(&t.content);
            }
        }
        pending.extend(ui.tree().layout_tree.children(node).into_iter().rev());
    }
    result
}

fn shown(ui: &Headless) -> String {
    text_at(ui, ui.find(".cn-date-picker-value").unwrap())
}

fn day_center(ui: &Headless, day: u32) -> (f32, f32) {
    let mut pending = vec![ui.find(".cn-calendar").unwrap()];
    while let Some(node) = pending.pop() {
        let registry = ui.tree().element_registry();
        if registry.has_class(node, "cn-calendar-day")
            && registry.attribute(node, "data-outside").is_none()
            && text_at(ui, node) == day.to_string()
        {
            let b = ui.tree().get_absolute_bounds(node).unwrap();
            return (b.x + b.width / 2.0, b.y + b.height / 2.0);
        }
        pending.extend(ui.tree().layout_tree.children(node));
    }
    panic!("no day {day}");
}

#[test]
fn selecting_a_day_reports_the_value_and_closes_the_popover() {
    if !Headless::available() {
        return;
    }
    let mut s = scene(ColorScheme::Light, None);
    assert_eq!(shown(&s.ui), "Choose a date");
    s.ui.click(".cn-date-picker-trigger").settle();
    s.ui.leave().assert_snapshot("cn_date_picker_open_light");
    let (x, y) = day_center(&s.ui, 17);
    s.ui.move_to(x, y).mouse_down().mouse_up().settle();
    assert_eq!(
        s.date.try_get().flatten(),
        Some(CalendarDay::new(2026, 11, 17))
    );
    assert_eq!(shown(&s.ui), "2026-11-17");
    assert_eq!(
        s.changes.lock().unwrap().as_slice(),
        &[Some(CalendarDay::new(2026, 11, 17))]
    );
    assert!(s.ui.find(".cn-date-picker-content").is_none());
    s.ui.leave()
        .assert_snapshot("cn_date_picker_selected_light");
}

#[test]
fn external_selection_and_placeholder_updates_are_silent() {
    if !Headless::available() {
        return;
    }
    let mut s = scene(ColorScheme::Dark, None);
    let label = s.ui.find(".cn-date-picker-value");
    s.placeholder.set("Departure".into());
    s.ui.settle();
    assert_eq!(shown(&s.ui), "Departure");
    s.ui.click(".cn-date-picker-trigger").settle();
    s.date.set(Some(CalendarDay::new(2026, 11, 20)));
    s.ui.settle();
    assert_eq!(shown(&s.ui), "2026-11-20");
    assert_eq!(s.ui.find(".cn-date-picker-value"), label);
    assert!(s.ui.find(".cn-date-picker-content").is_some());
    assert!(s.changes.lock().unwrap().is_empty());
    s.ui.leave().assert_snapshot("cn_date_picker_open_dark");
}

#[test]
fn clearing_returns_none_once_and_closes_the_popover() {
    if !Headless::available() {
        return;
    }
    let mut s = scene(ColorScheme::Dark, Some(TODAY));
    s.ui.click(".cn-date-picker-trigger")
        .settle()
        .click(".cn-date-picker-clear")
        .settle();
    assert_eq!(s.date.try_get().flatten(), None);
    assert_eq!(shown(&s.ui), "Choose a date");
    assert_eq!(s.changes.lock().unwrap().as_slice(), &[None]);
    assert!(s.ui.find(".cn-date-picker-content").is_none());
}

#[test]
fn reselecting_the_same_day_is_silent_and_dismisses() {
    if !Headless::available() {
        return;
    }
    let mut s = scene(ColorScheme::Dark, Some(TODAY));
    s.ui.click(".cn-date-picker-trigger").settle();
    let (x, y) = day_center(&s.ui, 3);
    s.ui.move_to(x, y).mouse_down().mouse_up().settle();
    assert!(s.changes.lock().unwrap().is_empty());
    assert!(s.ui.find(".cn-date-picker-content").is_none());
}

#[test]
fn outside_click_and_disabling_dismiss_without_changing_the_date() {
    if !Headless::available() {
        return;
    }
    let mut s = scene(ColorScheme::Light, Some(TODAY));
    s.ui.click(".cn-date-picker-trigger").settle();
    s.ui.move_to(600.0, 550.0).mouse_down().mouse_up().settle();
    assert!(s.ui.find(".cn-date-picker-content").is_none());
    s.ui.click(".cn-date-picker-trigger").settle();
    s.disabled.set(true);
    s.ui.settle();
    assert!(s.ui.find(".cn-date-picker-content").is_none());
    s.ui.click(".cn-date-picker-trigger").settle();
    assert!(s.ui.find(".cn-date-picker-content").is_none());
    assert_eq!(s.date.try_get().flatten(), Some(TODAY));
    assert!(s.changes.lock().unwrap().is_empty());
    s.ui.leave().assert_snapshot("cn_date_picker_disabled");
}

#[test]
fn calendar_grid_can_be_reached_and_selected_by_keyboard() {
    if !Headless::available() {
        return;
    }
    let mut s = scene(ColorScheme::Dark, None);
    s.ui.key(9).key(13).settle();
    assert!(s.ui.find(".cn-date-picker-content").is_some());
    for _ in 0..8 {
        s.ui.key(9).settle();
        if s.ui.focused() == s.ui.find(".cn-calendar-body") {
            break;
        }
    }
    assert_eq!(s.ui.focused(), s.ui.find(".cn-calendar-body"));
    s.ui.key(39).settle();
    s.ui.assert_snapshot("cn_date_picker_keyboard_focus");
    s.ui.key(13).settle();
    assert_eq!(s.date.try_get().flatten(), Some(TODAY.add_days(1)));
    assert!(s.ui.find(".cn-date-picker-content").is_none());
    assert_eq!(s.changes.lock().unwrap().len(), 1);
}

#[test]
fn custom_format_and_optional_clear_reuse_calendar_selection() {
    if !Headless::available() {
        return;
    }
    let mut s = scene(ColorScheme::Light, Some(TODAY));
    let date = s.date;
    s.ui.mount(move |_| {
        div().p(8.0).child(
            date_picker(date)
                .today(TODAY)
                .clearable(false)
                .format(|day| format!("{:02}/{:02}/{}", day.day, day.month, day.year)),
        )
    })
    .settle();
    assert_eq!(shown(&s.ui), "03/11/2026");
    s.ui.click(".cn-date-picker-trigger").settle();
    assert!(s.ui.find(".cn-date-picker-clear").is_none());
    let (x, y) = day_center(&s.ui, 25);
    s.ui.move_to(x, y).mouse_down().mouse_up().settle();
    assert_eq!(shown(&s.ui), "25/11/2026");
}

#[test]
fn month_dropdown_keeps_the_date_popover_open() {
    if !Headless::available() {
        return;
    }
    let mut s = scene(ColorScheme::Light, None);
    s.ui.click(".cn-date-picker-trigger")
        .settle()
        .click(".cn-select-trigger")
        .settle();
    let mut pending = vec![s.ui.tree().root().unwrap()];
    let mut december = None;
    while let Some(node) = pending.pop() {
        if s.ui
            .tree()
            .element_registry()
            .has_class(node, "cn-select-item")
            && text_at(&s.ui, node) == "December"
        {
            december = s.ui.tree().get_absolute_bounds(node);
            break;
        }
        pending.extend(s.ui.tree().layout_tree.children(node));
    }
    let b = december.expect("December option");
    s.ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0)
        .mouse_down()
        .mouse_up()
        .settle();
    assert!(
        s.ui.find(".cn-date-picker-content").is_some(),
        "a child dropdown dismissed its parent picker"
    );
    assert!(
        s.changes.lock().unwrap().is_empty(),
        "changing the viewed month is not a date selection"
    );
    let (x, y) = day_center(&s.ui, 15);
    s.ui.move_to(x, y).mouse_down().mouse_up().settle();
    assert_eq!(
        s.date.try_get().flatten(),
        Some(CalendarDay::new(2026, 12, 15))
    );
}
