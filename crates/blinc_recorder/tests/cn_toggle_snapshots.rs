#![cfg(feature = "headless")]
//! How a cn toggle and a toggle group look off and on, hovered, pressed and
//! disabled, in each variant and size.

use blinc_cn::{ToggleSize, ToggleVariant, toggle, toggle_group, toggle_item};
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 200;
const H: u32 = 64;

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

fn mount_toggle(
    ui: &mut Headless,
    on: bool,
    disabled: bool,
    variant: ToggleVariant,
    size: ToggleSize,
) -> State<bool> {
    let state = State::new(signal::<bool>(on), global_graph(), global_dirty_flag());
    let bound = state.clone();
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .items_center()
            .justify_center()
            .child(
                toggle(&bound)
                    .variant(variant)
                    .size(size)
                    .disabled(disabled)
                    .label("Bold"),
            )
    });
    state
}

#[test]
fn off_and_on_in_every_state() {
    let Some(mut ui) = session() else { return };
    for (name, on) in [("off", false), ("on", true)] {
        mount_toggle(
            &mut ui,
            on,
            false,
            ToggleVariant::Default,
            ToggleSize::Medium,
        );
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_toggle_{name}_idle"));

        ui.move_to(W as f32 / 2.0, H as f32 / 2.0).settle();
        ui.assert_snapshot(&format!("cn_toggle_{name}_hover"));

        // Held down, not released: a release would toggle it.
        ui.move_to(W as f32 / 2.0, H as f32 / 2.0)
            .mouse_down()
            .settle();
        ui.assert_snapshot(&format!("cn_toggle_{name}_pressed"));
        ui.leave().release();

        mount_toggle(
            &mut ui,
            on,
            true,
            ToggleVariant::Default,
            ToggleSize::Medium,
        );
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_toggle_{name}_disabled"));
    }
}

#[test]
fn variants_and_sizes() {
    let Some(mut ui) = session() else { return };
    for (name, on, variant, size) in [
        (
            "outline_off",
            false,
            ToggleVariant::Outline,
            ToggleSize::Medium,
        ),
        (
            "outline_on",
            true,
            ToggleVariant::Outline,
            ToggleSize::Medium,
        ),
        ("small_on", true, ToggleVariant::Default, ToggleSize::Small),
        ("large_on", true, ToggleVariant::Default, ToggleSize::Large),
        (
            "small_outline",
            false,
            ToggleVariant::Outline,
            ToggleSize::Small,
        ),
    ] {
        mount_toggle(&mut ui, on, false, variant, size);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_toggle_{name}"));
    }
}

#[test]
fn a_click_turns_it_on() {
    let Some(mut ui) = session() else { return };
    let state = mount_toggle(
        &mut ui,
        false,
        false,
        ToggleVariant::Default,
        ToggleSize::Medium,
    );
    ui.leave().settle();
    ui.move_to(W as f32 / 2.0, H as f32 / 2.0)
        .mouse_down()
        .advance(80);
    ui.release();
    assert!(state.get(), "the click did not turn it on");
    ui.advance(32);
    ui.assert_snapshot("cn_toggle_click_midway");
    ui.settle();
    ui.assert_snapshot("cn_toggle_click_after");
}

fn mount_group(ui: &mut Headless, variant: ToggleVariant, size: ToggleSize, disable_b: bool) {
    let state = State::new(
        signal::<String>("a".to_string()),
        global_graph(),
        global_dirty_flag(),
    );
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .items_center()
            .justify_center()
            .child(
                toggle_group(&state)
                    .variant(variant)
                    .size(size)
                    .item(toggle_item("a").label("Left"))
                    .item(toggle_item("b").label("Mid").disabled(disable_b))
                    .item(toggle_item("c").label("Right")),
            )
    });
}

#[test]
fn a_toggle_group() {
    let Some(mut ui) = session() else { return };
    for (name, variant, size, disable_b) in [
        ("default", ToggleVariant::Default, ToggleSize::Medium, false),
        (
            "outline_small",
            ToggleVariant::Outline,
            ToggleSize::Small,
            false,
        ),
        (
            "one_disabled",
            ToggleVariant::Default,
            ToggleSize::Medium,
            true,
        ),
    ] {
        mount_group(&mut ui, variant, size, disable_b);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_toggle_group_{name}_idle"));
        // The pointer on the middle item.
        ui.move_to(W as f32 / 2.0, H as f32 / 2.0).settle();
        ui.assert_snapshot(&format!("cn_toggle_group_{name}_hover"));
        ui.leave();
    }
}
