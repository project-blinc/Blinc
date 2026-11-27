#![cfg(feature = "headless")]
//! The cn builders take a class and an id, which reach the element, and a labelled switch answers
//! a click on its label.

use blinc_cn::{button, checkbox, switch, toggle};
use blinc_core::reactive::signal;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

#[test]
fn builders_take_a_class_and_an_id() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(320, 240).unwrap();
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    let (checked, on, pressed) = (signal(false), signal(false), signal(false));
    ui.mount(move |_| {
        div()
            .w(320.0)
            .h(240.0)
            .flex_col()
            .gap(2.0)
            .child(button("Save").class("mine").id("a-button"))
            .child(checkbox(checked).class("mine").id("a-checkbox"))
            .child(switch(on).label("Wifi").class("mine").id("a-switch"))
            .child(toggle(pressed).label("Bold").class("mine").id("a-toggle"))
    });
    ui.leave().settle();

    for id in ["#a-button", "#a-checkbox", "#a-switch", "#a-toggle"] {
        let node = ui.find(id).unwrap_or_else(|| panic!("no {id}"));
        assert!(
            ui.tree().element_registry().has_class(node, "mine"),
            "{id} has no class"
        );
    }

    let track = ui.find("#a-switch").unwrap();
    assert!(
        ui.tree().element_registry().has_class(track, "cn-switch"),
        "the switch names its track"
    );

    ui.click(".cn-switch-label").settle();
    assert!(on.get(), "the label toggles the switch");
}
