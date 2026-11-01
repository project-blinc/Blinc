#![cfg(feature = "headless")]
//! How a cn switch moves: the thumb keeps its place while the switch is held
//! down and eases both ways on release, and hovering the track lightens it.

use blinc_cn::switch;
use blinc_core::{Brush, Transform};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

fn session(on: bool) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(120, 60).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui.mount(move |ctx| {
        let state = ctx.use_state_keyed("switch_motion", move || on);
        div()
            .w(120.0)
            .h(60.0)
            .items_center()
            .justify_center()
            .child(switch(state.signal()))
    });
    ui.leave().settle();
    Some(ui)
}

/// The thumb's `(translate x, scale)` as painted.
fn thumb(ui: &Headless) -> (f32, f32) {
    let node = ui.find(".cn-switch-thumb").expect("the thumb");
    match &ui.tree().get_render_node(node).unwrap().props.transform {
        Some(Transform::Affine2D(a)) => (a.elements[4], a.elements[0]),
        _ => (0.0, 1.0),
    }
}

fn track_luma(ui: &Headless) -> f32 {
    let node = ui.find(".cn-switch").expect("the track");
    match &ui.tree().get_render_node(node).unwrap().props.background {
        Some(Brush::Solid(c)) => c.r + c.g + c.b,
        other => panic!("track fill is not solid: {other:?}"),
    }
}

#[test]
fn a_held_on_switch_keeps_its_thumb_in_place() {
    let Some(mut ui) = session(true) else { return };
    let (rest, _) = thumb(&ui);
    assert!(rest > 10.0, "the thumb is not at the end: {rest}");
    ui.press(".cn-switch").settle();
    let (held, scale) = thumb(&ui);
    assert!(
        (held - rest).abs() < 0.01,
        "pressing moved the thumb from {rest} to {held}"
    );
    assert!(scale < 0.95, "the thumb did not shrink: {scale}");
}

#[test]
fn the_thumb_eases_both_ways() {
    for on in [false, true] {
        let Some(mut ui) = session(on) else { return };
        let (start, _) = thumb(&ui);
        ui.press(".cn-switch").advance(80);
        ui.release();
        let mut seen = Vec::new();
        for _ in 0..24 {
            ui.advance(16);
            seen.push(thumb(&ui).0);
        }
        let end = seen[seen.len() - 1];
        let (lo, hi) = (start.min(end), start.max(end));
        assert!(hi - lo > 10.0, "the thumb did not travel: {start} -> {end}");
        assert!(
            seen.iter().any(|x| *x > lo + 2.0 && *x < hi - 2.0),
            "switching {} jumped without easing: {seen:?}",
            if on { "off" } else { "on" }
        );
    }
}

#[test]
fn hovering_an_off_track_lightens_it() {
    let Some(mut ui) = session(false) else { return };
    let idle = track_luma(&ui);
    ui.hover(".cn-switch").settle();
    let hovered = track_luma(&ui);
    assert!(hovered > idle, "hover {hovered} is not above idle {idle}");
    ui.leave().settle();
    assert!(
        (track_luma(&ui) - idle).abs() < 0.01,
        "leaving did not restore it"
    );
}

#[test]
fn switching_off_under_the_pointer_fades_the_track() {
    let Some(mut ui) = session(true) else { return };
    ui.hover(".cn-switch").settle();
    let on = track_luma(&ui);
    ui.press(".cn-switch").advance(80);
    ui.release();
    let mut seen = Vec::new();
    for _ in 0..30 {
        ui.advance(16);
        seen.push(track_luma(&ui));
    }
    let off = seen[seen.len() - 1];
    assert!(
        on - off > 0.5,
        "the track did not leave its on colour: {on} -> {off}"
    );
    assert!(
        seen.iter().any(|l| *l < on - 0.1 && *l > off + 0.1),
        "the track jumped to its off colour: {seen:?}"
    );
    // It stays where the pointer leaves it: no second change once it is there.
    let settled = seen[seen.len() - 6..].to_vec();
    assert!(
        settled.iter().all(|l| (l - off).abs() < 0.01),
        "the track moved again after fading: {settled:?}"
    );
}
