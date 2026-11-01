#![cfg(feature = "headless")]
//! How a cn checkbox looks unchecked and checked, hovered, pressed and
//! disabled, with and without a label.

use blinc_cn::{CheckboxSize, checkbox};
use blinc_core::reactive::{Signal, signal};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 140;
const H: u32 = 48;

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(W, H).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

fn mount(
    ui: &mut Headless,
    on: bool,
    disabled: bool,
    size: CheckboxSize,
    label: bool,
) -> Signal<bool> {
    mount_bound(ui, on, signal(disabled), size, label)
}

fn mount_bound(
    ui: &mut Headless,
    on: bool,
    disabled: Signal<bool>,
    size: CheckboxSize,
    label: bool,
) -> Signal<bool> {
    let checked = signal(on);
    ui.mount(move |_| {
        let mut cb = checkbox(checked).size(size).disabled(disabled);
        if label {
            cb = cb.label("Accept");
        }
        div()
            .w(W as f32)
            .h(H as f32)
            .items_center()
            .justify_center()
            .child(cb)
    });
    checked
}

#[test]
fn unchecked_and_checked_in_every_state() {
    let Some(mut ui) = session() else { return };
    for (name, on) in [("unchecked", false), ("checked", true)] {
        mount(&mut ui, on, false, CheckboxSize::Medium, true);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_checkbox_{name}_idle"));

        ui.hover(".cn-checkbox").settle();
        ui.assert_snapshot(&format!("cn_checkbox_{name}_hover"));

        ui.press(".cn-checkbox").settle();
        ui.assert_snapshot(&format!("cn_checkbox_{name}_pressed"));
        ui.release().leave();

        mount(&mut ui, on, true, CheckboxSize::Medium, true);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_checkbox_{name}_disabled"));
    }
}

#[test]
fn sizes_when_checked() {
    let Some(mut ui) = session() else { return };
    for (name, size) in [
        ("small", CheckboxSize::Small),
        ("large", CheckboxSize::Large),
    ] {
        mount(&mut ui, true, false, size, false);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_checkbox_{name}_checked"));
    }
}

#[test]
fn a_click_checks_it() {
    let Some(mut ui) = session() else { return };
    let checked = mount(&mut ui, false, false, CheckboxSize::Medium, true);
    ui.leave().settle();
    ui.press(".cn-checkbox").advance(80);
    ui.release();
    assert!(checked.get(), "the click did not check it");
    ui.advance(32);
    ui.assert_snapshot("cn_checkbox_click_midway");
    ui.settle();
    ui.assert_snapshot("cn_checkbox_click_after");
}

#[test]
fn a_bound_disabled_follows_its_signal_in_place() {
    let Some(mut ui) = session() else { return };
    let disabled = signal(false);
    let checked = mount_bound(&mut ui, false, disabled, CheckboxSize::Medium, true);
    ui.leave().settle();

    disabled.set(true);
    ui.settle();
    ui.assert_snapshot("cn_checkbox_unchecked_disabled");
    ui.click(".cn-checkbox").settle();
    assert!(!checked.get(), "a disabled checkbox was checked");

    disabled.set(false);
    ui.leave().settle();
    ui.assert_snapshot("cn_checkbox_unchecked_idle");
    ui.click(".cn-checkbox").settle();
    assert!(checked.get(), "re-enabled, the click did not check it");
}
