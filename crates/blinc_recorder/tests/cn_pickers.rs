#![cfg(feature = "headless")]
//! Picker interaction and appearance through the offscreen renderer.

#[path = "cn_pickers/time_wheel.rs"]
mod time_wheel;

use blinc_cn::{TimeFormat, TimeOfDay, color_picker, time_picker};
use blinc_core::{
    Color,
    events::event_types,
    reactive::{Signal, signal},
};
use blinc_layout::{div::div, event_handler::EventContext, renderer::ElementType};
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};
use std::sync::{Arc, Mutex};

fn session(scheme: ColorScheme) -> Headless {
    let mut ui = Headless::new(640, 520).expect("offscreen GPU adapter");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        scheme,
    );
    ui
}

fn colors() -> Vec<Color> {
    [
        0xef4444, 0xf59e0b, 0xeab308, 0x22c55e, 0x14b8a6, 0x3b82f6, 0x6366f1, 0xec4899, 0xa855f7,
        0x111827, 0xffffff,
    ]
    .into_iter()
    .map(Color::from_hex)
    .collect()
}

fn text(ui: &Headless, selector: &str) -> String {
    let tree = ui.tree();
    let mut nodes = vec![ui.find(selector).expect(selector)];
    let mut result = String::new();
    while let Some(n) = nodes.pop() {
        if let Some(node) = tree.get_render_node(n) {
            if let ElementType::Text(t) = &node.element_type {
                result.push_str(&t.content);
            }
        }
        nodes.extend(tree.layout_tree.children(n).into_iter().rev());
    }
    result
}

fn replace_text(ui: &mut Headless, selector: &str, value: &str) {
    select_all(ui, selector);
    let node = ui.find(selector).expect(selector);
    let stable = ui.tree().stable_id(node).expect("field stable id");
    for ch in value.chars() {
        let mut e = EventContext::new(event_types::TEXT_INPUT, node).with_key_char(ch);
        e.stable_id = stable;
        ui.tree().handler_registry().dispatch(&e);
        ui.frame(0);
    }
    ui.settle();
}

fn select_all(ui: &mut Headless, selector: &str) {
    ui.click(selector);
    let node = ui.find(selector).expect(selector);
    let stable = ui.tree().stable_id(node).expect("field stable id");
    let mut select = EventContext::new(event_types::KEY_DOWN, node)
        .with_key_code(65)
        .with_modifiers(false, true, false, false);
    select.stable_id = stable;
    ui.tree().handler_registry().dispatch(&select);
    ui.settle();
}

fn mount_time(
    ui: &mut Headless,
    format: TimeFormat,
    seconds: bool,
) -> (Signal<TimeOfDay>, Signal<bool>, Arc<Mutex<Vec<TimeOfDay>>>) {
    let value = signal(TimeOfDay::new(23, 58, 45).unwrap());
    let disabled = signal(false);
    let changes = Arc::new(Mutex::new(Vec::new()));
    let seen = changes.clone();
    ui.mount(move |_| {
        let seen = seen.clone();
        div().p(8.0).child(
            time_picker(value)
                .format(format)
                .seconds(seconds)
                .minute_step(5)
                .disabled(disabled)
                .on_change(move |v| seen.lock().unwrap().push(v)),
        )
    })
    .settle();
    (value, disabled, changes)
}

#[test]
fn time_fields_wrap_and_preserve_unedited_parts() {
    let mut ui = session(ColorScheme::Dark);
    let (value, _, changes) = mount_time(&mut ui, TimeFormat::TwentyFourHour, true);
    ui.click(".cn-time-picker-hour").key(38).settle();
    assert_eq!(value.get(), TimeOfDay::new(0, 58, 45).unwrap());
    assert_eq!(text(&ui, ".cn-time-picker-hour"), "00");
    ui.click(".cn-time-picker-minute").key(38).settle();
    assert_eq!(value.get().minute(), 3);
    assert_eq!(changes.lock().unwrap().len(), 2);
    let node = ui.find(".cn-time-picker-hour");
    value.set(TimeOfDay::new(7, 12, 3).unwrap());
    ui.settle();
    assert_eq!(ui.find(".cn-time-picker-hour"), node);
    assert_eq!(text(&ui, ".cn-time-picker-hour"), "07");
    assert_eq!(text(&ui, ".cn-time-picker-minute"), "12");
    assert_eq!(text(&ui, ".cn-time-picker-second"), "03");
    assert_eq!(
        changes.lock().unwrap().len(),
        2,
        "external writes are silent"
    );
    ui.leave().assert_snapshot("cn_time_picker_24_dark");
}

#[test]
fn twelve_hour_noon_midnight_and_hidden_seconds() {
    let mut ui = session(ColorScheme::Light);
    let (value, _, _) = mount_time(&mut ui, TimeFormat::TwelveHour, false);
    value.set(TimeOfDay::new(0, 30, 45).unwrap());
    ui.settle();
    assert_eq!(text(&ui, ".cn-time-picker-hour"), "12");
    assert_eq!(text(&ui, ".cn-time-picker-period"), "AM");
    ui.click(".cn-time-picker-period").settle();
    assert_eq!(value.get(), TimeOfDay::new(12, 30, 45).unwrap());
    replace_text(&mut ui, ".cn-time-picker-hour", "01");
    ui.key(13).settle();
    assert_eq!(value.get(), TimeOfDay::new(13, 30, 45).unwrap());
    assert_eq!(text(&ui, ".cn-time-picker-period"), "PM");
    ui.leave().assert_snapshot("cn_time_picker_12_light");
}

#[test]
fn disabled_time_rejects_keys_and_period_clicks() {
    let mut ui = session(ColorScheme::Dark);
    let (value, disabled, changes) = mount_time(&mut ui, TimeFormat::TwelveHour, true);
    ui.click(".cn-time-picker-hour");
    disabled.set(true);
    ui.settle();
    ui.key(38).click(".cn-time-picker-period").settle();
    assert_eq!(value.get(), TimeOfDay::new(23, 58, 45).unwrap());
    assert!(changes.lock().unwrap().is_empty());
    ui.leave().assert_snapshot("cn_time_picker_disabled");
}

struct ColorScene {
    value: Signal<Color>,
    disabled: Signal<bool>,
    swatches: Signal<Vec<Color>>,
    changes: Arc<Mutex<Vec<Color>>>,
}

fn mount_color(ui: &mut Headless, inline: bool, alpha: bool) -> ColorScene {
    let value = signal(Color::from_hex(0x6366f1));
    let disabled = signal(false);
    let swatches = signal(colors());
    let changes = Arc::new(Mutex::new(Vec::new()));
    let seen = changes.clone();
    ui.mount(move |_| {
        let seen = seen.clone();
        let picker = color_picker(value)
            .alpha(alpha)
            .disabled(disabled)
            .swatches(swatches)
            .on_change(move |v| seen.lock().unwrap().push(v));
        div()
            .p(8.0)
            .child(if inline { picker.inline() } else { picker })
    })
    .settle();
    ColorScene {
        value,
        disabled,
        swatches,
        changes,
    }
}

#[test]
fn color_panel_light_snapshot() {
    let mut ui = session(ColorScheme::Light);
    let s = mount_color(&mut ui, true, true);
    s.value.set(s.value.get().with_alpha(0.55));
    ui.settle();
    let area = ui
        .tree()
        .get_absolute_bounds(ui.find(".cn-color-picker-sv").unwrap())
        .unwrap();
    let marker = ui
        .tree()
        .get_absolute_bounds(ui.find(".cn-color-picker-marker").unwrap())
        .unwrap();
    let (_, saturation, brightness, _) = s.value.get().to_hsva();
    assert!((marker.x + marker.width / 2.0 - area.x - saturation * area.width).abs() < 1.0);
    assert!(
        (marker.y + marker.height / 2.0 - area.y - (1.0 - brightness) * area.height).abs() < 1.0
    );
    ui.leave().assert_snapshot("cn_color_picker_inline_light");
}

#[test]
fn color_plane_drag_clamps_and_external_writes_follow_in_place() {
    let mut ui = session(ColorScheme::Dark);
    let s = mount_color(&mut ui, true, true);
    let node = ui.find(".cn-color-picker-sv").unwrap();
    let b = ui.tree().get_absolute_bounds(node).unwrap();
    ui.move_to(b.x + b.width * 0.5, b.y + b.height * 0.25)
        .mouse_down()
        .move_to(b.x + b.width + 20.0, b.y - 10.0)
        .mouse_up()
        .settle();
    let (_, saturation, brightness, _) = s.value.get().to_hsva();
    assert!(saturation > 0.99 && brightness > 0.99);
    let count = s.changes.lock().unwrap().len();
    s.value.set(Color::from_hex(0x22c55e));
    ui.settle();
    assert_eq!(ui.find(".cn-color-picker-sv"), Some(node));
    assert_eq!(text(&ui, ".cn-color-picker-hex"), "#22c55eff");
    assert_eq!(s.changes.lock().unwrap().len(), count);
    ui.leave().assert_snapshot("cn_color_picker_inline_dark");
}

#[test]
fn hue_survives_black_and_alpha_responds_to_keys() {
    let mut ui = session(ColorScheme::Dark);
    let s = mount_color(&mut ui, true, true);
    ui.click(".cn-color-picker-hue").key(35).settle();
    let hue = s.value.get().to_hsva().0;
    let sv = ui.find(".cn-color-picker-sv").unwrap();
    let b = ui.tree().get_absolute_bounds(sv).unwrap();
    ui.move_to(b.x + b.width - 1.0, b.y + b.height - 1.0)
        .mouse_down()
        .move_to(b.x + b.width, b.y + b.height + 10.0)
        .mouse_up();
    ui.click(".cn-color-picker-hue").key(36).key(39);
    ui.move_to(b.x + b.width - 1.0, b.y + 1.0)
        .mouse_down()
        .move_to(b.x + b.width, b.y)
        .mouse_up()
        .settle();
    assert!(
        (s.value.get().to_hsva().0 - 1.0).abs() < 0.1,
        "hue retained while black, was {hue}"
    );
    ui.click(".cn-color-picker-alpha").key(36).key(39).settle();
    assert!((s.value.get().a - 0.01).abs() < 0.0001);
}

#[test]
fn hex_entry_commits_on_enter_and_reports_invalid_input() {
    let mut ui = session(ColorScheme::Dark);
    let s = mount_color(&mut ui, true, true);
    replace_text(&mut ui, ".cn-color-picker-hex", "#f008");
    assert_eq!(
        s.value.get(),
        Color::from_hex(0x6366f1),
        "typing remains a draft"
    );
    ui.key(13).settle();
    assert_eq!(s.value.get(), Color::from_hex_str("#ff000088").unwrap());
    assert_eq!(s.changes.lock().unwrap().len(), 1);
    replace_text(&mut ui, ".cn-color-picker-hex", "#oops");
    ui.key(13).settle();
    assert_eq!(s.changes.lock().unwrap().len(), 1);
    assert!(ui.find(".cn-color-picker-hex--invalid").is_some());
    s.value.set(Color::BLUE);
    ui.settle();
    assert_eq!(text(&ui, ".cn-color-picker-hex"), "#0000ffff");
    assert!(ui.find(".cn-color-picker-hex--invalid").is_none());
}

#[test]
fn palette_updates_and_disabled_controls_preserve_the_value() {
    let mut ui = session(ColorScheme::Dark);
    let s = mount_color(&mut ui, true, false);
    s.value.set(s.value.get().with_alpha(0.4));
    ui.settle();
    s.swatches.set(vec![Color::RED]);
    ui.settle();
    ui.click(".cn-color-picker-preset").settle();
    assert_eq!(s.value.get(), Color::RED.with_alpha(0.4));
    let preset = ui.find(".cn-color-picker-preset").unwrap();
    assert_eq!(
        ui.tree()
            .element_registry()
            .attribute(preset, "data-state")
            .as_deref(),
        Some("selected")
    );
    assert_eq!(s.changes.lock().unwrap().len(), 1);
    s.disabled.set(true);
    ui.settle();
    ui.click(".cn-color-picker-sv")
        .key(38)
        .click(".cn-color-picker-hue")
        .key(35)
        .click(".cn-color-picker-preset")
        .settle();
    assert_eq!(s.value.get(), Color::RED.with_alpha(0.4));
    assert_eq!(s.changes.lock().unwrap().len(), 1);
    ui.leave().assert_snapshot("cn_color_picker_disabled");
}

#[test]
fn popover_opens_follows_value_and_dismisses_outside() {
    let mut ui = session(ColorScheme::Dark);
    let s = mount_color(&mut ui, false, true);
    ui.click(".cn-color-picker-trigger").settle();
    assert!(ui.find(".cn-color-picker-panel").is_some());
    s.value.set(Color::MAGENTA.with_alpha(0.5));
    ui.settle();
    assert_eq!(text(&ui, ".cn-color-picker-value"), "#ff00ff80");
    assert_eq!(text(&ui, ".cn-color-picker-hex"), "#ff00ff80");
    ui.leave().assert_snapshot("cn_color_picker_popover_dark");
    ui.move_to(600.0, 480.0).mouse_down().mouse_up().settle();
    assert!(ui.find(".cn-color-picker-panel").is_none());
    s.disabled.set(true);
    ui.settle();
    ui.click(".cn-color-picker-trigger").settle();
    assert!(ui.find(".cn-color-picker-panel").is_none());
}

#[test]
fn disabling_open_popover_closes_it() {
    let mut ui = session(ColorScheme::Light);
    let s = mount_color(&mut ui, false, false);
    ui.click(".cn-color-picker-trigger").settle();
    assert!(ui.find(".cn-color-picker-panel").is_some());
    s.disabled.set(true);
    ui.settle();
    assert!(ui.find(".cn-color-picker-panel").is_none());
}

#[test]
fn time_value_rejects_invalid_times_and_formats_boundaries() {
    assert!(TimeOfDay::new(24, 0, 0).is_none());
    assert!(TimeOfDay::new(23, 60, 0).is_none());
    assert!(TimeOfDay::new(23, 59, 60).is_none());
    assert_eq!(
        TimeOfDay::new(0, 5, 9)
            .unwrap()
            .format(TimeFormat::TwelveHour, true),
        "12:05:09 AM"
    );
    assert_eq!(
        TimeOfDay::new(12, 0, 0)
            .unwrap()
            .format(TimeFormat::TwelveHour, false),
        "12:00 PM"
    );
}

fn time_gallery(scheme: ColorScheme, name: &str) {
    use blinc_cn::{InputSize, LabelSize};
    let mut ui = Headless::new(640, 320).expect("offscreen GPU adapter");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        scheme,
    );
    let time = signal(TimeOfDay::new(2, 30, 15).unwrap());
    ui.mount(move |_| {
        div().p(6.0).flex_col().gap(5.0).children(
            [InputSize::Small, InputSize::Medium, InputSize::Large]
                .into_iter()
                .map(|size| {
                    let caption = match size {
                        InputSize::Small => "Small",
                        InputSize::Medium => "Medium",
                        InputSize::Large => "Large",
                    };
                    div()
                        .flex_col()
                        .gap(2.0)
                        .child(blinc_cn::label(caption).size(LabelSize::Small))
                        .child(
                            div()
                                .flex_row()
                                .items_center()
                                .gap(6.0)
                                .child(time_picker(time).size(size).seconds(true))
                                .child(time_picker(time).size(size).format(TimeFormat::TwelveHour))
                                .child(time_picker(time).size(size).disabled(true)),
                        )
                }),
        )
    })
    .leave()
    .settle();
    ui.assert_snapshot(name);
}

#[test]
fn time_picker_sizes_dark() {
    time_gallery(ColorScheme::Dark, "cn_time_picker_sizes_dark");
}

#[test]
fn time_picker_sizes_light() {
    time_gallery(ColorScheme::Light, "cn_time_picker_sizes_light");
}

#[test]
fn time_selection_stops_highlighting_after_blur() {
    let mut ui = session(ColorScheme::Dark);
    mount_time(&mut ui, TimeFormat::TwentyFourHour, true);
    let highlighted = |ui: &Headless, selector: &str| {
        let field = ui.find(selector).unwrap();
        let mut pending = ui.tree().layout_tree.children(field);
        while let Some(node) = pending.pop() {
            let visible_box = ui.tree().get_render_node(node).is_some_and(
                |n| matches!(&n.props.background, Some(blinc_core::Brush::Solid(c)) if c.a > 0.0),
            ) && ui
                .tree()
                .get_absolute_bounds(node)
                .is_some_and(|b| b.width > 1.0 && b.height > 1.0);
            if visible_box {
                return true;
            }
            pending.extend(ui.tree().layout_tree.children(node));
        }
        false
    };
    select_all(&mut ui, ".cn-time-picker-hour");
    assert!(highlighted(&ui, ".cn-time-picker-hour"));
    ui.click(".cn-time-picker-minute").settle();
    assert!(
        !highlighted(&ui, ".cn-time-picker-hour"),
        "a blurred field still draws its selection"
    );
    select_all(&mut ui, ".cn-time-picker-minute");
    assert!(highlighted(&ui, ".cn-time-picker-minute"));
    ui.key(27).settle();
    assert!(!highlighted(&ui, ".cn-time-picker-minute"));
    let root = ui.find(".cn-time-picker").unwrap();
    assert!(
        ui.tree()
            .element_registry()
            .attribute(root, "data-focus-within")
            .is_none()
    );
    ui.leave()
        .settle()
        .assert_snapshot("cn_time_picker_blurred");
}

#[test]
fn hue_strip_renders_green_and_blue() {
    let mut ui = session(ColorScheme::Light);
    mount_color(&mut ui, true, false);
    let b = ui
        .tree()
        .get_absolute_bounds(ui.find(".cn-color-picker-hue").unwrap())
        .unwrap();
    let frame = ui.snapshot().unwrap();
    let sample = |t: f32| {
        frame
            .get_pixel((b.x + b.width * t) as u32, (b.y + b.height / 2.0) as u32)
            .unwrap()
    };
    let green = sample(1.0 / 3.0);
    let blue = sample(2.0 / 3.0);
    assert!(
        green[1] > 200 && green[0] < 60 && green[2] < 60,
        "green hue: {green:?}"
    );
    assert!(
        blue[2] > 200 && blue[0] < 60 && blue[1] < 60,
        "blue hue: {blue:?}"
    );
}
