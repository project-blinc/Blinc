#![cfg(feature = "headless")]
//! What the stylesheet passes cost with the cn stylesheet loaded and many components built.
//! Not checks: the numbers. Run with `--run-ignored only --no-capture`.

use blinc_cn::{badge, button, checkbox, switch};
use blinc_core::reactive::signal;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

fn count() -> usize {
    std::env::var("STYLE_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(400)
}

fn session() -> Option<Headless> {
    if !Headless::available() {
        return None;
    }
    let mut ui = Headless::new(900, 700).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui.gate_state_styles();
    Some(ui)
}

/// Milliseconds to build `STYLE_NODES` groups of a button, a badge, a checkbox and a switch, and
/// to rebuild what one toggled switch rebuilds.
#[test]
#[ignore]
fn what_the_style_passes_cost() {
    let Some(mut ui) = session() else {
        return;
    };
    let groups = count();
    let first = signal(false);
    let flags: Vec<_> = (0..groups).map(|_| signal(false)).collect();
    let started = std::time::Instant::now();
    ui.mount(move |_| {
        let mut list = div().flex_col().w(880.0).gap(1.0);
        for (i, flag) in flags.iter().enumerate() {
            list = list.child(
                div()
                    .flex_row()
                    .gap(2.0)
                    .child(button(format!("Button {i}")))
                    .child(badge("new"))
                    .child(checkbox(*flag))
                    .child(switch(if i == 0 { first } else { *flag })),
            );
        }
        div().w(900.0).h(700.0).child(list)
    });
    ui.settle();
    let build = started.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "{groups} groups ({} nodes): build {build:.0} ms",
        ui.tree().layout_tree.len()
    );

    // A change that rebuilds a subtree: the first switch.
    let mut costs = Vec::new();
    let toggles: usize = std::env::var("STYLE_TOGGLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    eprintln!("READY toggling");
    for step in 0..toggles {
        let started = std::time::Instant::now();
        first.set(step % 2 == 0);
        ui.frame(16);
        costs.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    costs.sort_by(|a, b| a.total_cmp(b));
    eprintln!(
        "  one switch toggled, frame ms: median {:.2}  max {:.2}",
        costs[costs.len() / 2],
        costs[costs.len() - 1]
    );
}
