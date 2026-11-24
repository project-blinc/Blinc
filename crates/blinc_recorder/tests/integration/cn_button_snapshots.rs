#![cfg(feature = "headless")]
//! How a cn button looks in each variant and state, recorded as images.
//!
//! `BLINC_SNAPSHOT=bless cargo nextest run -p blinc_recorder --features headless`
//! records them; the images are in `.blinc/snapshots/blinc_recorder/<os>/`.

use blinc_cn::{ButtonVariant, button};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 168;
const H: u32 = 64;

fn mount(ui: &mut Headless, variant: ButtonVariant, disabled: bool) {
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .items_center()
            .justify_center()
            .child(button("Save").variant(variant).disabled(disabled))
    });
}

#[test]
fn every_variant_in_every_state() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(W, H).expect("headless");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    for (name, variant) in [
        ("primary", ButtonVariant::Primary),
        ("secondary", ButtonVariant::Secondary),
        ("destructive", ButtonVariant::Destructive),
        ("outline", ButtonVariant::Outline),
        ("ghost", ButtonVariant::Ghost),
        ("link", ButtonVariant::Link),
    ] {
        mount(&mut ui, variant, false);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_button_{name}_idle"));

        ui.hover(".cn-button").settle();
        ui.assert_snapshot(&format!("cn_button_{name}_hover"));

        ui.press(".cn-button").settle();
        ui.assert_snapshot(&format!("cn_button_{name}_pressed"));
        ui.release().leave();

        mount(&mut ui, variant, true);
        ui.leave().settle();
        ui.assert_snapshot(&format!("cn_button_{name}_disabled"));
    }
}

#[test]
fn a_hover_part_way_through_its_transition() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(W, H).expect("headless");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    mount(&mut ui, ButtonVariant::Primary, false);
    ui.leave().settle();
    ui.hover(".cn-button").advance(64);
    ui.assert_snapshot("cn_button_primary_hover_midway");
}
