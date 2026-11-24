#![cfg(feature = "headless")]
//! A tooltip takes no input and does not cover its trigger: by a window edge it opens on the
//! side that has room, and the trigger keeps the pointer and its clicks. Pressing the trigger,
//! leaving it, or waiting closes the tooltip, including when the press hides the trigger.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use blinc_cn::prelude::*;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

#[test]
fn a_tooltip_by_the_top_edge_opens_below_and_leaves_its_trigger_clickable() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(400, 200).unwrap();
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    let clicks = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&clicks);
    ui.mount(move |_| {
        let counted = Arc::clone(&counted);
        div().w_full().h_full().child(
            tooltip(move || {
                let counted = Arc::clone(&counted);
                div().id("trigger").w(24.0).h(24.0).on_click(move |_| {
                    counted.fetch_add(1, Ordering::SeqCst);
                })
            })
            .text("Close the panel")
            .side(TooltipSide::Top)
            .open_delay_ms(0),
        )
    });
    ui.settle();

    ui.hover("#trigger");
    ui.advance(400).settle();
    let tip = ui.find(".cn-tooltip").expect("hovering opened no tooltip");
    let trigger = ui.find("#trigger").unwrap();
    let (tip_b, trigger_b) = (
        ui.tree().get_absolute_bounds(tip).unwrap(),
        ui.tree().get_absolute_bounds(trigger).unwrap(),
    );
    assert!(
        tip_b.y >= trigger_b.y + trigger_b.height,
        "with no room above, the tooltip did not open below its trigger: {tip_b:?} over {trigger_b:?}"
    );

    ui.click("#trigger");
    ui.settle();
    assert_eq!(
        clicks.load(Ordering::SeqCst),
        1,
        "the trigger lost its click"
    );
    assert!(
        ui.find(".cn-tooltip").is_none(),
        "pressing the trigger left its tooltip open"
    );

    ui.move_to(300.0, 150.0);
    ui.hover("#trigger");
    ui.advance(400).settle();
    assert!(ui.find(".cn-tooltip").is_some());
    ui.move_to(300.0, 150.0);
    ui.advance(400).settle();
    assert!(
        ui.find(".cn-tooltip").is_none(),
        "leaving the trigger left the tooltip open"
    );
}

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(400, 200).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

#[test]
fn a_trigger_its_press_hides_takes_its_tooltip_with_it() {
    let Some(mut ui) = session() else { return };
    let shown = blinc_core::reactive::signal(true);
    ui.mount(move |_| {
        div().w_full().h_full().pt(20.0).child(
            div().visible(shown).child(
                tooltip(move || {
                    div()
                        .id("close")
                        .w(24.0)
                        .h(24.0)
                        .on_click(move |_| shown.set(false))
                })
                .text("Close the panel")
                .open_delay_ms(0),
            ),
        )
    });
    ui.settle();
    ui.hover("#close");
    ui.advance(400).settle();
    assert!(ui.find(".cn-tooltip").is_some());
    ui.click("#close");
    ui.advance(400).settle();
    assert!(!shown.get(), "the click did not reach the trigger");
    assert!(
        ui.find(".cn-tooltip").is_none(),
        "the tooltip outlived its hidden trigger"
    );
}

#[test]
fn a_tooltip_closes_by_itself_after_a_while() {
    let Some(mut ui) = session() else { return };
    ui.mount(move |_| {
        div().w_full().h_full().pt(20.0).child(
            tooltip(|| div().id("trigger").w(24.0).h(24.0))
                .text("Close the panel")
                .open_delay_ms(0)
                .auto_hide_ms(1_000),
        )
    });
    ui.settle();
    ui.hover("#trigger");
    ui.advance(400).settle();
    assert!(ui.find(".cn-tooltip").is_some());
    ui.advance(1_200).settle();
    assert!(
        ui.find(".cn-tooltip").is_none(),
        "the tooltip stayed past its auto-hide with the pointer still on the trigger"
    );
}
