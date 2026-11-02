#![cfg(feature = "headless")]
//! A cn progress bar takes its colours from rules, eases to a bound value
//! in place, and pulses while indeterminate.

use blinc_cn::{progress, progress_indeterminate};
use blinc_core::reactive::signal;
use blinc_core::{Brush, Color};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

const W: u32 = 240;
const H: u32 = 40;
const TRACK: f32 = 200.0;

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

fn background(ui: &Headless, selector: &str) -> Option<Color> {
    let node = ui.find(selector).expect(selector);
    match &ui.tree().get_render_node(node).unwrap().props.background {
        Some(Brush::Solid(c)) => Some(*c),
        _ => None,
    }
}

fn close(a: Option<Color>, b: Color) -> bool {
    a.is_some_and(|a| {
        (a.r - b.r).abs() < 0.02 && (a.g - b.g).abs() < 0.02 && (a.b - b.b).abs() < 0.02
    })
}

fn bar_width(ui: &Headless) -> f32 {
    let bar = ui.find(".cn-progress-bar").expect("the bar");
    ui.tree().get_absolute_bounds(bar).unwrap().width
}

#[test]
fn a_bound_value_eases_in_place() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    let done = signal(25.0f32);
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(20.0))
            .child(progress(done).w(TRACK))
    });
    ui.leave().settle();
    assert!(close(
        background(&ui, ".cn-progress"),
        theme.color(ColorToken::Border)
    ));
    assert!(close(
        background(&ui, ".cn-progress-bar"),
        theme.color(ColorToken::Primary)
    ));
    assert!((bar_width(&ui) - 0.25 * TRACK).abs() < 1.0);
    let bar = ui.find(".cn-progress-bar").unwrap();
    ui.assert_snapshot("cn_progress_quarter");

    done.set(75.0);
    ui.advance(100);
    let partway = bar_width(&ui);
    assert!(
        partway > 0.25 * TRACK + 5.0 && partway < 0.75 * TRACK - 5.0,
        "the bar jumped: {partway}"
    );
    ui.settle();
    assert!(
        (bar_width(&ui) - 0.75 * TRACK).abs() < 1.0,
        "{}",
        bar_width(&ui)
    );
    assert_eq!(
        ui.find(".cn-progress-bar"),
        Some(bar),
        "the bar was rebuilt"
    );
}

#[test]
fn an_indeterminate_bar_pulses() {
    let Some(mut ui) = session() else { return };
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(20.0))
            .child(progress_indeterminate().w(TRACK))
    });
    ui.advance(100);
    let bar = ui.find(".cn-progress-bar").unwrap();
    assert!((bar_width(&ui) - 0.3 * TRACK).abs() < 1.0);
    // The bar is composited as a layer, so its opacity is read from the
    // animation rather than its props.
    let opacity = |ui: &Headless| {
        let tree = ui.tree();
        let id = tree.stable_id(bar).unwrap();
        let store = tree.css_anim_store();
        let store = store.lock().unwrap();
        let pulse = store.animations.get(&id).expect("no pulse");
        assert_eq!(pulse.name.as_deref(), Some("cn-progress-pulse"));
        pulse.current_properties.opacity.unwrap_or(1.0)
    };
    let first = opacity(&ui);
    ui.advance(300);
    let later = opacity(&ui);
    assert!(
        (first - later).abs() > 0.05,
        "the bar did not pulse: {first} then {later}"
    );
}
