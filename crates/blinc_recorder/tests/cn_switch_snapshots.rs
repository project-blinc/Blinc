#![cfg(feature = "headless")]
//! How a cn switch looks off and on, hovered, pressed and disabled.
//!
//! `BLINC_SNAPSHOT=bless cargo nextest run -p blinc_recorder --features headless`
//! records the images in `.blinc/snapshots/blinc_recorder/<os>/`.

use blinc_cn::{SwitchSize, switch};
use blinc_core::reactive::{Signal, signal};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 96;
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

/// `disabled` as a constant.
fn mount(ui: &mut Headless, on: bool, disabled: bool, size: SwitchSize) -> Signal<bool> {
    let state = signal(on);
    ui.mount(move |_| host(switch(state).size(size).disabled(disabled)));
    state
}

/// `disabled` bound to a signal.
fn mount_bound(
    ui: &mut Headless,
    on: bool,
    disabled: Signal<bool>,
    size: SwitchSize,
) -> Signal<bool> {
    let state = signal(on);
    ui.mount(move |_| host(switch(state).size(size).disabled(disabled)));
    state
}

fn host(switch: blinc_cn::switch::SwitchBuilder) -> blinc_layout::div::Div {
    div()
        .w(W as f32)
        .h(H as f32)
        .items_center()
        .justify_center()
        .child(switch)
}

#[test]
fn off_and_on_in_every_state() {
    let Some(mut ui) = session() else { return };
    for (name, on) in [("off", false), ("on", true)] {
        mount(&mut ui, on, false, SwitchSize::Medium);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_switch_{name}_idle"));

        ui.hover(".cn-switch").settle();
        ui.assert_snapshot(&format!("cn_switch_{name}_hover"));

        ui.press(".cn-switch").settle();
        ui.assert_snapshot(&format!("cn_switch_{name}_pressed"));
        ui.leave();

        mount(&mut ui, on, true, SwitchSize::Medium);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_switch_{name}_disabled"));
    }
}

#[test]
fn sizes_when_on() {
    let Some(mut ui) = session() else { return };
    for (name, size) in [("small", SwitchSize::Small), ("large", SwitchSize::Large)] {
        mount(&mut ui, true, false, size);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_switch_{name}_on"));
    }
}

#[test]
fn a_click_moves_the_thumb_across() {
    let Some(mut ui) = session() else { return };
    let state = mount(&mut ui, false, false, SwitchSize::Medium);
    ui.leave().settle();
    ui.assert_snapshot("cn_switch_click_before");
    ui.press(".cn-switch").advance(80);
    ui.release();
    assert!(state.get(), "the click did not switch it on");
    ui.advance(48);
    ui.assert_snapshot("cn_switch_click_midway");
    ui.settle();
    ui.leave().settle();
    ui.assert_snapshot("cn_switch_click_after");
}

#[test]
fn a_bound_disabled_follows_its_signal_in_place() {
    let Some(mut ui) = session() else { return };
    let disabled = signal(false);
    let on = mount_bound(&mut ui, false, disabled, SwitchSize::Medium);
    ui.leave().settle();

    disabled.set(true);
    ui.settle();
    ui.assert_snapshot("cn_switch_off_disabled");
    ui.click(".cn-switch").settle();
    assert!(!on.get(), "a disabled switch was switched on");

    disabled.set(false);
    ui.leave().settle();
    ui.assert_snapshot("cn_switch_off_idle");
    ui.click(".cn-switch").settle();
    assert!(on.get(), "re-enabled, the click did not switch it on");
}

/// A constant and a bound `disabled` cascade alike under `:checked`.
#[test]
fn on_and_disabled_looks_the_same_bound_or_constant() {
    let Some(mut ui) = session() else { return };
    mount_bound(&mut ui, true, signal(true), SwitchSize::Medium);
    ui.leave().settle();
    ui.assert_snapshot("cn_switch_on_disabled");
}
