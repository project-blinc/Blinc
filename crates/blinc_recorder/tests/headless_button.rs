#![cfg(feature = "headless")]
//! A cn button built, hovered and recorded offscreen.

use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(240, 100).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui.mount(|_| {
        blinc_layout::div::div()
            .w(240.0)
            .h(100.0)
            .items_center()
            .justify_center()
            .child(blinc_cn::button("Save"))
    });
    Some(ui)
}

#[test]
fn hover_changes_the_painted_fill() {
    let Some(mut ui) = session() else { return };
    ui.settle();
    let idle = ui.snapshot().expect("idle");
    let (cx, cy) = ui.centre(".cn-button");
    let at = |f: &blinc_recorder::CapturedFrame| f.get_pixel(cx as u32 - 30, cy as u32).unwrap();
    ui.hover(".cn-button").settle();
    let hover = ui.snapshot().expect("hover");
    assert_ne!(at(&idle), at(&hover), "hover did not change the fill");
}

#[test]
fn a_click_is_recorded_with_the_tree_it_acted_on() {
    let Some(mut ui) = session() else { return };
    ui.start_recording();
    ui.click(".cn-button").settle();
    let export = ui.stop_recording().expect("a recording");
    assert!(
        export.stats.total_events > 0,
        "a click recorded no events: {:?}",
        export.stats
    );
    assert!(export.stats.total_snapshots > 0, "no tree snapshots");
    assert!(
        export
            .events
            .iter()
            .any(|e| matches!(e.event, blinc_recorder::RecordedEvent::Click(_))),
        "no click in {:?}",
        export.events.iter().map(|e| &e.event).collect::<Vec<_>>()
    );
    let last = export.snapshots.last().unwrap();
    assert!(
        last.elements.values().any(|e| e.is_hovered),
        "the snapshot does not show the hover"
    );
}

#[test]
fn a_recorded_session_replays_to_the_same_frame() {
    let Some(mut ui) = session() else { return };
    ui.settle();
    ui.start_recording();
    ui.hover(".cn-button").advance(120);
    let export = ui.stop_recording().expect("a recording");
    let (cx, cy) = ui.centre(".cn-button");
    let at = |f: &blinc_recorder::CapturedFrame| f.get_pixel(cx as u32 - 30, cy as u32).unwrap();
    let recorded = ui.settle().snapshot().expect("recorded");

    let Some(mut again) = session() else { return };
    again.settle();
    let idle = again.snapshot().expect("idle");
    assert_ne!(at(&idle), at(&recorded));
    again.replay(&export);
    let replayed = again.snapshot().expect("replayed");
    assert_eq!(
        at(&replayed),
        at(&recorded),
        "the replay did not reproduce the hover"
    );
}
