//! Property-driven region updates apply layout selectors as the native frame runner does.

use super::super::Headless;
use blinc_core::reactive::signal;
use blinc_layout::div::div;
use blinc_theme::{ColorScheme, HybridTheme};

fn marker_x(ui: &Headless) -> f32 {
    ui.tree()
        .get_absolute_bounds(ui.find("#marker").unwrap())
        .unwrap()
        .x
}

#[test]
fn region_updates_reapply_ancestor_layout_selectors() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(160, 80).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark)
        .gate_state_styles();
    ui.css(".parity-host { padding: 0px; } .parity-host:has(.extra) { padding-left: 24px; }");
    let shown = signal(false);
    ui.mount(move |_| {
        div()
            .class("parity-host")
            .w(160.0)
            .h(80.0)
            .flex_col()
            .child(div().id("marker").w(10.0).h(10.0))
            .show(shown, || div().class("extra").w(10.0).h(10.0))
    });
    let before = marker_x(&ui);
    shown.set(true);
    ui.apply_queued_updates();
    assert_eq!(
        marker_x(&ui),
        before + 24.0,
        ":has layout did not follow insertion"
    );
    shown.set(false);
    ui.apply_queued_updates();
    assert_eq!(marker_x(&ui), before, ":has layout did not follow removal");
}

#[test]
fn region_updates_reapply_sibling_layout_selectors() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(160, 80).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark)
        .gate_state_styles();
    ui.css(
        ".parity-target { margin-left: 0px; }
         .parity-target:first-child { margin-left: 12px; }
         .extra + .parity-target { margin-left: 32px; }",
    );
    let shown = signal(false);
    ui.mount(move |_| {
        div()
            .w(160.0)
            .h(80.0)
            .flex_row()
            .show(shown, || div().class("extra").w(10.0).h(10.0))
            .child(div().id("marker").class("parity-target").w(10.0).h(10.0))
    });
    let before = marker_x(&ui);
    shown.set(true);
    ui.apply_queued_updates();
    assert_eq!(
        marker_x(&ui),
        before + 30.0,
        "sibling layout did not follow insertion"
    );
    shown.set(false);
    ui.apply_queued_updates();
    assert_eq!(
        marker_x(&ui),
        before,
        "sibling layout did not follow removal"
    );
}
