#![cfg(feature = "headless")]
//! Colour-field integration with keyboard focus traversal.

use blinc_cn::{TimeFormat, TimeOfDay};
use blinc_core::{Color, reactive::signal};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

#[test]
fn color_surface_is_reachable_and_editable_by_tab() {
    let mut ui = Headless::new(640, 520).expect("offscreen GPU adapter");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Light,
    );
    let color = signal(Color::from_hex(0x6366f1));
    ui.mount(move |_| {
        div()
            .p(8.0)
            .child(blinc_cn::color_picker(color).inline().alpha(true))
    })
    .settle();
    ui.key(9).settle();
    assert_eq!(ui.focused(), ui.find(".cn-color-picker-sv"));
    let node = ui.find(".cn-color-picker-sv").unwrap();
    let props = &ui.tree().get_render_node(node).unwrap().props;
    assert!(props.outline_width > 0.0 && props.outline_color.is_some());
    ui.assert_snapshot("cn_color_picker_keyboard_focus");
    let before = color.get();
    ui.key(39).settle();
    assert_ne!(
        color.get(),
        before,
        "the tab-focused colour field did not receive the arrow key"
    );
}

#[test]
fn tab_transfers_arrow_keys_between_color_fields_and_hex() {
    let mut ui = Headless::new(640, 520).expect("offscreen GPU adapter");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    let color = signal(Color::from_hsva(210.0, 0.6, 0.8, 1.0));
    ui.mount(move |_| {
        div()
            .p(8.0)
            .child(blinc_cn::color_picker(color).inline().alpha(true))
    })
    .settle();
    ui.click(".cn-color-picker-sv").key(9).settle();
    assert_eq!(ui.focused(), ui.find(".cn-color-picker-hue"));
    let (h, s, v, a) = color.get().to_hsva();
    ui.key(39).settle();
    let (next_h, next_s, next_v, next_a) = color.get().to_hsva();
    assert!((next_h - h - 1.0).abs() < 0.001);
    assert!((next_s - s).abs() < 0.001 && (next_v - v).abs() < 0.001 && next_a == a);
    ui.key(9).key(37).settle();
    assert_eq!(ui.focused(), ui.find(".cn-color-picker-alpha"));
    assert!((color.get().a - 0.99).abs() < 0.001);
    let before = color.get();
    ui.key(9).key(37).key(38).settle();
    assert_eq!(ui.focused(), ui.find(".cn-color-picker-hex"));
    assert_eq!(
        color.get(),
        before,
        "hex navigation must not edit a previously focused field"
    );
}

#[test]
fn disabled_color_fields_ignore_keys_and_tab_skips_them() {
    let mut ui = Headless::new(640, 520).expect("offscreen GPU adapter");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Light,
    );
    let color = signal(Color::BLUE);
    let disabled = signal(false);
    ui.mount(move |_| {
        div()
            .p(8.0)
            .flex_col()
            .child(
                blinc_cn::color_picker(color)
                    .inline()
                    .alpha(true)
                    .disabled(disabled),
            )
            .child(blinc_cn::button("After"))
    })
    .settle();
    ui.key(9);
    disabled.set(true);
    ui.settle();
    ui.key(39).key(9).settle();
    assert_eq!(color.get(), Color::BLUE);
    assert_eq!(ui.focused(), ui.find(".cn-button"));
}

#[test]
fn time_fields_and_period_are_editable_by_tab() {
    let mut ui = Headless::new(640, 240).expect("offscreen GPU adapter");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    let time = signal(TimeOfDay::new(14, 30, 15).unwrap());
    ui.mount(move |_| {
        div().p(8.0).child(
            blinc_cn::time_picker(time)
                .format(TimeFormat::TwelveHour)
                .seconds(true)
                .minute_step(5),
        )
    })
    .settle();
    ui.key(9).key(38).settle();
    assert_eq!(ui.focused(), ui.find(".cn-time-picker-hour"));
    assert_eq!(time.get().hour(), 15);
    ui.key(9).key(38).settle();
    assert_eq!(ui.focused(), ui.find(".cn-time-picker-minute"));
    assert_eq!(time.get().minute(), 35);
    ui.key(9).key(40).settle();
    assert_eq!(ui.focused(), ui.find(".cn-time-picker-second"));
    assert_eq!(time.get().second(), 14);
    ui.key(9).key(32).settle();
    assert_eq!(ui.focused(), ui.find(".cn-time-picker-period"));
    assert_eq!(time.get().hour(), 3);
    ui.key_with_shift(9, true).key(38).settle();
    assert_eq!(time.get(), TimeOfDay::new(3, 35, 15).unwrap());
    ui.assert_snapshot("cn_time_picker_keyboard_focus");
}
