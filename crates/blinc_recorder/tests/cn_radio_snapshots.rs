#![cfg(feature = "headless")]
//! How a cn radio group looks with the first option selected and not, hovered,
//! pressed and disabled.

use blinc_cn::{RadioSize, radio_group};
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 140;
const H: u32 = 96;

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
    first_selected: bool,
    disabled: bool,
    size: RadioSize,
) -> State<String> {
    let state = State::new(
        signal::<String>(if first_selected { "a" } else { "b" }.to_string()),
        global_graph(),
        global_dirty_flag(),
    );
    let bound = state.clone();
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .items_center()
            .justify_center()
            .child(
                radio_group(bound.signal())
                    .size(size)
                    .disabled(disabled)
                    .option("a", "First")
                    .option("b", "Second"),
            )
    });
    state
}

#[test]
fn selected_and_unselected_in_every_state() {
    let Some(mut ui) = session() else { return };
    for (name, first_selected) in [("selected", true), ("unselected", false)] {
        mount(&mut ui, first_selected, false, RadioSize::Medium);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_radio_{name}_idle"));

        ui.hover(".cn-radio").settle();
        ui.assert_snapshot(&format!("cn_radio_{name}_hover"));

        // Held down, not released: a release would select it.
        ui.press(".cn-radio").settle();
        ui.assert_snapshot(&format!("cn_radio_{name}_pressed"));
        ui.leave().release();

        mount(&mut ui, first_selected, true, RadioSize::Medium);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_radio_{name}_disabled"));
    }
}

#[test]
fn sizes() {
    let Some(mut ui) = session() else { return };
    for (name, size) in [("small", RadioSize::Small), ("large", RadioSize::Large)] {
        mount(&mut ui, true, false, size);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_radio_{name}"));
    }
}

#[test]
fn a_click_selects_it() {
    let Some(mut ui) = session() else { return };
    let state = mount(&mut ui, false, false, RadioSize::Medium);
    ui.leave().settle();
    // The first radio is the unselected one: click it.
    ui.press(".cn-radio").advance(80);
    ui.release();
    assert_eq!(state.get(), "a", "the click did not select it");
    ui.advance(32);
    ui.assert_snapshot("cn_radio_click_midway");
    ui.settle().leave().settle();
    ui.assert_snapshot("cn_radio_click_after");
}
