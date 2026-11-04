use super::session;
use blinc_cn::{TimeFormat, TimeOfDay, time_picker};
use blinc_core::reactive::{Signal, signal};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::ColorScheme;
use std::sync::{Arc, Mutex};

struct Scene {
    ui: Headless,
    time: Signal<TimeOfDay>,
    disabled: Signal<bool>,
    changes: Arc<Mutex<Vec<TimeOfDay>>>,
}

fn open(format: TimeFormat, seconds: bool, scheme: ColorScheme) -> Scene {
    open_with_steps(format, seconds, scheme, (1, 1))
}

fn open_with_steps(
    format: TimeFormat,
    seconds: bool,
    scheme: ColorScheme,
    steps: (u8, u8),
) -> Scene {
    let mut ui = session(scheme);
    let time = signal(TimeOfDay::new(3, 12, 15).unwrap());
    let disabled = signal(false);
    let changes = Arc::new(Mutex::new(Vec::new()));
    let seen = changes.clone();
    ui.mount(move |_| {
        let seen = seen.clone();
        div().p(8.0).child(
            time_picker(time)
                .format(format)
                .seconds(seconds)
                .minute_step(steps.0)
                .second_step(steps.1)
                .disabled(disabled)
                .on_change(move |value| seen.lock().unwrap().push(value)),
        )
    })
    .settle();
    ui.click(".cn-time-picker-clock").settle();
    assert!(ui.find(".cn-time-picker-wheel").is_some());
    Scene {
        ui,
        time,
        disabled,
        changes,
    }
}

fn wheel(ui: &mut Headless, selector: &str, delta: f32) {
    let b = ui
        .tree()
        .get_absolute_bounds(ui.find(selector).unwrap())
        .unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0)
        .scroll(0.0, delta * b.height / 5.0 / 40.0)
        .settle();
}

fn choose_row(ui: &mut Headless, selector: &str, row: f32) {
    let b = ui
        .tree()
        .get_absolute_bounds(ui.find(selector).unwrap())
        .unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height * (row + 0.5) / 5.0)
        .mouse_down()
        .mouse_up()
        .settle();
}

#[test]
fn wheel_picker_light_and_separate_period() {
    let mut s = open(TimeFormat::TwelveHour, false, ColorScheme::Light);
    s.ui.leave()
        .settle()
        .assert_snapshot("cn_time_picker_wheel_12_light");
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-hour-current"), "03");
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-minute-current"), "12");
    let popup =
        s.ui.tree()
            .get_absolute_bounds(s.ui.find(".cn-time-picker-wheel").unwrap())
            .unwrap();
    assert!(popup.width <= 234.0 && popup.height <= 300.0, "{popup:?}");
    assert_edge_fade(&mut s.ui, ".cn-time-picker-wheel-hour");
    s.ui.click(".cn-time-picker-wheel-pm").settle();
    assert_eq!(s.time.get().hour(), 3);
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-hour-current"), "03");
    s.ui.click(".cn-time-picker-wheel-save").settle();
    assert_eq!(s.time.get(), TimeOfDay::new(15, 12, 15).unwrap());
    assert_eq!(s.changes.lock().unwrap().len(), 1);
    assert!(s.ui.find(".cn-time-picker-wheel").is_none());
}

#[test]
fn scrolling_columns_keeps_edits_provisional_until_save() {
    let mut s = open(TimeFormat::TwentyFourHour, true, ColorScheme::Dark);
    s.ui.leave()
        .settle()
        .assert_snapshot("cn_time_picker_wheel_24_dark");
    wheel(&mut s.ui, ".cn-time-picker-wheel-hour", 40.0);
    wheel(&mut s.ui, ".cn-time-picker-wheel-minute", -80.0);
    wheel(&mut s.ui, ".cn-time-picker-wheel-second", 40.0);
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-hour-current"), "04");
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-minute-current"), "10");
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-second-current"), "16");
    assert_eq!(s.time.get(), TimeOfDay::new(3, 12, 15).unwrap());
    assert!(s.changes.lock().unwrap().is_empty());
    s.ui.assert_snapshot("cn_time_picker_wheel_scrolling_dark");
    s.ui.leave()
        .settle()
        .assert_snapshot("cn_time_picker_wheel_scrolled_dark");
    let selection =
        s.ui.tree()
            .get_absolute_bounds(s.ui.find(".cn-time-picker-wheel-selection").unwrap())
            .unwrap();
    for class in [
        ".cn-time-picker-wheel-hour-current",
        ".cn-time-picker-wheel-minute-current",
        ".cn-time-picker-wheel-second-current",
    ] {
        let row =
            s.ui.tree()
                .get_absolute_bounds(current_row(&s.ui, class))
                .unwrap();
        assert!(
            (row.y + row.height / 2.0 - selection.y - selection.height / 2.0).abs() < 0.5,
            "{class}: {row:?}, selection: {selection:?}"
        );
    }
    s.ui.click(".cn-time-picker-wheel-save").settle();
    assert_eq!(s.time.get(), TimeOfDay::new(4, 10, 16).unwrap());
    assert_eq!(*s.changes.lock().unwrap(), [s.time.get()]);
}

#[test]
fn cancel_escape_and_outside_dismissal_discard_the_draft() {
    let mut s = open(TimeFormat::TwentyFourHour, false, ColorScheme::Dark);
    for action in ["cancel", "escape", "outside"] {
        wheel(&mut s.ui, ".cn-time-picker-wheel-minute", 40.0);
        match action {
            "cancel" => {
                s.ui.click(".cn-time-picker-wheel-cancel").settle();
            }
            "escape" => {
                blinc_layout::overlay_state::overlay_stack()
                    .lock()
                    .unwrap()
                    .handle_escape();
                blinc_layout::widgets::overlay_stack::drain_close_callbacks();
                s.ui.settle();
            }
            _ => {
                s.ui.move_to(600.0, 490.0).mouse_down().mouse_up().settle();
            }
        }
        assert_eq!(s.time.get(), TimeOfDay::new(3, 12, 15).unwrap());
        assert!(s.changes.lock().unwrap().is_empty());
        assert!(
            s.ui.find(".cn-time-picker-wheel").is_none(),
            "{action} did not close the popup"
        );
        s.ui.click(".cn-time-picker-clock").settle();
        assert_eq!(text(&s.ui, ".cn-time-picker-wheel-minute-current"), "12");
    }
    s.ui.click(".cn-time-picker-wheel-save").settle();
    assert!(s.changes.lock().unwrap().is_empty());
}

#[test]
fn native_scroll_tracks_small_deltas_without_changing_other_columns() {
    let mut s = open(TimeFormat::TwentyFourHour, false, ColorScheme::Dark);
    wheel(&mut s.ui, ".cn-time-picker-wheel-minute", 15.0);
    wheel(&mut s.ui, ".cn-time-picker-wheel-minute", 15.0);
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-minute-current"), "13");
    wheel(&mut s.ui, ".cn-time-picker-wheel-minute", 15.0);
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-minute-current"), "13");
    wheel(&mut s.ui, ".cn-time-picker-wheel-minute", -20.0);
    wheel(&mut s.ui, ".cn-time-picker-wheel-minute", -20.0);
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-minute-current"), "12");
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-hour-current"), "03");
    assert_eq!(s.time.get().second(), 15);
}

#[test]
fn clicking_and_dragging_select_visible_rows() {
    let mut s = open(TimeFormat::TwentyFourHour, false, ColorScheme::Dark);
    choose_row(&mut s.ui, ".cn-time-picker-wheel-hour", 1.0);
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-hour-current"), "04");
    let b =
        s.ui.tree()
            .get_absolute_bounds(s.ui.find(".cn-time-picker-wheel-minute").unwrap())
            .unwrap();
    s.ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0)
        .mouse_down()
        .move_to(b.x + b.width / 2.0, b.y + b.height * 0.9)
        .mouse_up()
        .settle();
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-minute-current"), "14");
    s.ui.click(".cn-time-picker-wheel-save").settle();
    assert_eq!(s.time.get(), TimeOfDay::new(4, 14, 15).unwrap());
}

#[test]
fn external_writes_follow_and_keyboard_edits_save_once() {
    let mut s = open(TimeFormat::TwentyFourHour, false, ColorScheme::Dark);
    wheel(&mut s.ui, ".cn-time-picker-wheel-minute", 40.0);
    s.time.set(TimeOfDay::new(23, 59, 45).unwrap());
    s.ui.settle();
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-hour-current"), "23");
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-minute-current"), "59");
    s.ui.key(9).settle();
    assert_eq!(s.ui.focused(), s.ui.find(".cn-time-picker-wheel-hour"));
    s.ui.key(38).settle();
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-hour-current"), "00");
    s.ui.key(9).key(38).settle();
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-minute-current"), "00");
    s.ui.key(9).key(9).key(13).settle();
    assert_eq!(s.time.get(), TimeOfDay::new(0, 0, 45).unwrap());
    assert_eq!(s.changes.lock().unwrap().len(), 1);
}

#[test]
fn disabled_editor_closes_the_draft_and_popup_can_be_omitted() {
    let mut s = open(TimeFormat::TwentyFourHour, true, ColorScheme::Dark);
    wheel(&mut s.ui, ".cn-time-picker-wheel-minute", 40.0);
    s.disabled.set(true);
    s.ui.settle();
    assert!(s.ui.find(".cn-time-picker-wheel").is_none());
    s.ui.click(".cn-time-picker-clock").settle();
    assert!(s.ui.find(".cn-time-picker-wheel").is_none());
    assert!(s.changes.lock().unwrap().is_empty());
    let time = s.time;
    s.ui.mount(move |_| div().p(8.0).child(time_picker(time).popup(false)))
        .settle();
    assert!(s.ui.find(".cn-time-picker-clock").is_none());
}

#[test]
fn keyboard_steps_and_twelve_hour_wrap_preserve_the_period() {
    let mut s = open_with_steps(TimeFormat::TwelveHour, true, ColorScheme::Dark, (5, 10));
    s.time.set(TimeOfDay::new(23, 58, 55).unwrap());
    s.ui.settle();
    for class in [
        ".cn-time-picker-wheel-hour",
        ".cn-time-picker-wheel-minute",
        ".cn-time-picker-wheel-second",
    ] {
        for _ in 0..7 {
            if s.ui.focused() == s.ui.find(class) {
                break;
            }
            s.ui.key(9).settle();
        }
        assert_eq!(s.ui.focused(), s.ui.find(class));
        s.ui.key(38).settle();
    }
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-hour-current"), "12");
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-minute-current"), "03");
    assert_eq!(text(&s.ui, ".cn-time-picker-wheel-second-current"), "05");
    s.ui.click(".cn-time-picker-wheel-save").settle();
    assert_eq!(s.time.get(), TimeOfDay::new(12, 3, 5).unwrap());
    assert_eq!(s.changes.lock().unwrap().len(), 1);
}

#[test]
fn tab_reaches_separate_period_controls_columns_and_footer() {
    let mut s = open(TimeFormat::TwelveHour, false, ColorScheme::Dark);
    let stops = s.ui.tree().tab_stops();
    assert_eq!(stops.len(), 6);
    for node in &stops {
        s.ui.key(9).settle();
        assert_eq!(s.ui.focused(), Some(*node));
    }
    for node in stops[..5].iter().rev() {
        s.ui.key_with_shift(9, true).settle();
        assert_eq!(s.ui.focused(), Some(*node));
    }
    assert!(s.changes.lock().unwrap().is_empty());
}

#[test]
fn row_height_can_be_restyled_through_css() {
    use blinc_theme::HybridTheme;
    let mut ui = session(ColorScheme::Light);
    ui.theme(
        HybridTheme::bundle().with_css(format!(
            ":root {{ --cn-time-picker-wheel-row-height: 44px; }}\n{}",
            blinc_cn::cn_styles::CN_STYLES
        )),
        ColorScheme::Light,
    );
    let time = signal(TimeOfDay::new(3, 12, 0).unwrap());
    ui.mount(move |_| div().p(8.0).child(time_picker(time)))
        .settle();
    ui.click(".cn-time-picker-clock").settle();
    let bounds = |class: &str| {
        ui.tree()
            .get_absolute_bounds(if class.ends_with("-current") {
                current_row(&ui, class)
            } else {
                ui.find(class).unwrap()
            })
            .unwrap()
    };
    let viewport = bounds(".cn-time-picker-wheels");
    let selected = bounds(".cn-time-picker-wheel-selection");
    let hour = bounds(".cn-time-picker-wheel-hour-current");
    assert!(
        (viewport.height - 220.0).abs() < 0.5,
        "viewport: {viewport:?}"
    );
    assert!((selected.height - 44.0).abs() < 0.5);
    assert!((selected.y - viewport.y - 88.0).abs() < 0.5);
    assert!((hour.y - selected.y).abs() < 0.5);
}

fn assert_edge_fade(ui: &mut Headless, selector: &str) {
    let area = ui
        .tree()
        .get_absolute_bounds(ui.find(selector).unwrap())
        .unwrap();
    let frame = ui.snapshot().unwrap();
    let ink = |row: usize| {
        let top = area.y + row as f32 * area.height / 5.0;
        let bg = frame
            .get_pixel((area.x + 4.0) as u32, (top + area.height / 10.0) as u32)
            .unwrap();
        let mut contrast = 0u8;
        for y in (top + 3.0) as u32..(top + area.height / 5.0 - 3.0) as u32 {
            for x in
                (area.x + area.width / 2.0 - 18.0) as u32..(area.x + area.width / 2.0 + 18.0) as u32
            {
                let pixel = frame.get_pixel(x, y).unwrap();
                contrast = contrast.max(pixel[0].abs_diff(bg[0]));
            }
        }
        contrast as f32
    };
    let (outer, centre) = (ink(0), ink(2));
    assert!(
        centre > 100.0 && outer < centre * 0.65,
        "edge: {outer}, centre: {centre}"
    );
}

#[test]
fn overflow_fade_reaches_main_tree_text() {
    use blinc_core::Color;
    use blinc_layout::text::Text;
    let mut ui = session(ColorScheme::Light);
    ui.mount(|_| {
        div().w_full().h_full().child(
            div()
                .id("faded-numbers")
                .absolute()
                .left(80.0)
                .top(80.0)
                .w(100.0)
                .h(160.0)
                .overflow_y_scroll()
                .overflow_fade_y(64.0)
                .bg(Color::WHITE)
                .child(div().flex_col().children((0..5).map(|_| {
                    div()
                        .w_full()
                        .h(32.0)
                        .flex_shrink_0()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .child(Text::new("88").size(22.0).color(Color::BLACK))
                }))),
        )
    })
    .settle();
    assert_edge_fade(&mut ui, "#faded-numbers");
}

#[test]
fn popup_overlaps_its_control_and_stays_inside_the_window() {
    let mut ui = session(ColorScheme::Dark);
    let time = signal(TimeOfDay::new(3, 12, 0).unwrap());
    ui.mount(move |_| {
        div().w_full().h_full().child(
            div()
                .absolute()
                .left(260.0)
                .top(250.0)
                .child(time_picker(time).format(TimeFormat::TwelveHour)),
        )
    })
    .settle();
    let control = ui
        .tree()
        .get_absolute_bounds(ui.find(".cn-time-picker-clock").unwrap())
        .unwrap();
    ui.click(".cn-time-picker-clock").settle();
    let popup = ui
        .tree()
        .get_absolute_bounds(ui.find(".cn-time-picker-wheel").unwrap())
        .unwrap();
    assert!(popup.x < control.x + control.width && popup.x + popup.width > control.x);
    assert!(popup.y < control.y + control.height && popup.y + popup.height > control.y);
    assert!(
        popup.x >= 0.0
            && popup.y >= 0.0
            && popup.x + popup.width <= 640.0
            && popup.y + popup.height <= 520.0,
        "{popup:?}"
    );
    ui.leave()
        .settle()
        .assert_snapshot("cn_time_picker_wheel_over_control_dark");
}

fn current_row(ui: &Headless, selector: &str) -> blinc_layout::LayoutNodeId {
    let column = ui.find(selector.trim_end_matches("-current")).unwrap();
    let view = ui.tree().get_absolute_bounds(column).unwrap();
    let mut pending = vec![column];
    let mut nearest = None;
    while let Some(node) = pending.pop() {
        if ui
            .tree()
            .element_registry()
            .has_class(node, "cn-time-picker-wheel-option")
        {
            let bounds = ui.tree().get_absolute_bounds(node).unwrap();
            let distance = (bounds.y + bounds.height / 2.0 - view.y - view.height / 2.0).abs();
            if nearest.is_none_or(|(_, previous)| distance < previous) {
                nearest = Some((node, distance));
            }
        }
        pending.extend(ui.tree().layout_tree.children(node));
    }
    nearest.unwrap().0
}

fn text(ui: &Headless, selector: &str) -> String {
    if !selector.ends_with("-current") {
        return super::text(ui, selector);
    }
    let mut pending = vec![current_row(ui, selector)];
    while let Some(node) = pending.pop() {
        if let Some(blinc_layout::renderer::ElementType::Text(text)) = ui
            .tree()
            .get_render_node(node)
            .map(|node| &node.element_type)
        {
            return text.content.clone();
        }
        pending.extend(ui.tree().layout_tree.children(node));
    }
    panic!("selected row has no text");
}
