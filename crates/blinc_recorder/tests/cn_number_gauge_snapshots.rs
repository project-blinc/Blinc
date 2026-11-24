#![cfg(feature = "headless")]
//! How a cn number gauge looks and answers the pointer, the wheel and the
//! keyboard. Its colours are rules, so each state is checked against the theme
//! tokens its rule names as well as by snapshot.

use std::sync::{Arc, Mutex};

use blinc_cn::{GaugeSize, number_gauge};
use blinc_core::reactive::{Signal, signal};
use blinc_core::{Brush, Color};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

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

struct Mounted {
    value: Signal<f64>,
    changes: Arc<Mutex<Vec<f64>>>,
}

fn mount(ui: &mut Headless, initial: f64, disabled: bool) -> Mounted {
    mount_with(ui, initial, signal(disabled))
}

fn mount_with(ui: &mut Headless, initial: f64, disabled: Signal<bool>) -> Mounted {
    let changes = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&changes);
    let value = signal(initial);
    ui.mount(move |_| {
        let seen = Arc::clone(&seen);
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(12.0))
            .flex_col()
            .justify_center()
            .child(
                number_gauge(value)
                    .size(GaugeSize::Medium)
                    .disabled(disabled)
                    .on_change(move |v| seen.lock().unwrap().push(v)),
            )
    });
    ui.leave().settle();
    Mounted { value, changes }
}

fn bounds(ui: &Headless, selector: &str) -> blinc_layout::element::ElementBounds {
    let node = ui.find(selector).expect(selector);
    ui.tree().get_absolute_bounds(node).expect("bounds")
}

fn background(ui: &Headless, selector: &str) -> Option<Color> {
    let node = ui.find(selector).expect(selector);
    match &ui.tree().get_render_node(node).unwrap().props.background {
        Some(Brush::Solid(c)) => Some(*c),
        _ => None,
    }
}

fn border(ui: &Headless) -> Option<Color> {
    let node = ui.find(".cn-number-gauge").expect("the gauge");
    ui.tree().get_render_node(node).unwrap().props.border_color
}

fn close(a: Option<Color>, b: Color) -> bool {
    a.is_some_and(|a| {
        (a.r - b.r).abs() < 0.02 && (a.g - b.g).abs() < 0.02 && (a.b - b.b).abs() < 0.02
    })
}

fn mix(a: Color, b: Color, amount: f32) -> Color {
    Color::rgba(
        a.r * amount + b.r * (1.0 - amount),
        a.g * amount + b.g * (1.0 - amount),
        a.b * amount + b.b * (1.0 - amount),
        1.0,
    )
}

/// Press at `fraction` of the way along the grip's travel.
fn press_at(ui: &mut Headless, fraction: f32) {
    let g = bounds(ui, ".cn-number-gauge");
    let x = g.x + 1.0 + 5.5 + fraction * (g.width - 2.0 - 11.0);
    ui.move_to(x, g.y + g.height / 2.0).mouse_down();
}

fn has_attr(ui: &Headless, attr: &str) -> bool {
    let node = ui.find(".cn-number-gauge").expect("the gauge");
    ui.tree()
        .element_registry()
        .get_classes(node)
        .unwrap_or_default()
        .iter()
        .any(|c| &**c == attr)
}

#[test]
fn idle_and_hovered_follow_their_rules() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount(&mut ui, 40.0, false);
    let primary = theme.color(ColorToken::Primary);
    let input_bg = theme.color(ColorToken::InputBg);
    assert!(close(border(&ui), theme.color(ColorToken::Border)));
    assert!(close(
        background(&ui, ".cn-number-gauge-fill"),
        mix(primary, input_bg, 0.22)
    ));
    assert!(close(
        background(&ui, ".cn-number-gauge-thumb"),
        mix(primary, input_bg, 0.30)
    ));
    // The fill takes 40 % of the room the grip leaves.
    let (g, fill) = (
        bounds(&ui, ".cn-number-gauge"),
        bounds(&ui, ".cn-number-gauge-fill"),
    );
    let travel = g.width - 2.0 - 11.0;
    assert!(
        (fill.width - 0.4 * travel).abs() < 1.0,
        "fill {} of travel {travel}",
        fill.width
    );
    ui.assert_snapshot("cn_number_gauge_idle");

    ui.hover(".cn-number-gauge").settle();
    assert!(close(border(&ui), theme.color(ColorToken::BorderHover)));
    assert!(close(
        background(&ui, ".cn-number-gauge-thumb"),
        mix(primary, input_bg, 0.44)
    ));
    ui.assert_snapshot("cn_number_gauge_hover");
}

#[test]
fn the_width_follows_the_bounds_not_the_value() {
    let Some(mut ui) = session() else { return };
    let m = mount(&mut ui, 0.0, false);
    let before = bounds(&ui, ".cn-number-gauge").width;
    assert!(before >= 114.0, "narrower than the base width: {before}");
    m.value.set(100.0);
    ui.settle();
    assert_eq!(bounds(&ui, ".cn-number-gauge").width, before);
}

#[test]
fn a_press_seeks_and_a_drag_follows() {
    let Some(mut ui) = session() else { return };
    let m = mount(&mut ui, 40.0, false);
    press_at(&mut ui, 0.75);
    assert_eq!(m.value.get(), 75.0);
    assert!(
        has_attr(&ui, "data-state=dragging"),
        "not dragging while held"
    );

    let g = bounds(&ui, ".cn-number-gauge");
    ui.move_to(g.x + g.width + 40.0, g.y + g.height / 2.0);
    assert_eq!(m.value.get(), 100.0, "a drag past the end stops at max");
    ui.release().settle();
    assert!(
        !has_attr(&ui, "data-state=dragging"),
        "still dragging after release"
    );
    assert_eq!(*m.changes.lock().unwrap(), vec![75.0, 100.0]);
}

#[test]
fn the_wheel_steps_the_value() {
    let Some(mut ui) = session() else { return };
    let m = mount(&mut ui, 40.0, false);
    ui.hover(".cn-number-gauge");
    ui.scroll(0.0, 40.0);
    assert_eq!(m.value.get(), 41.0);
    // A partial line waits for the rest of the step.
    ui.scroll(0.0, -20.0);
    assert_eq!(m.value.get(), 41.0);
    ui.scroll(0.0, -20.0);
    assert_eq!(m.value.get(), 40.0);
}

#[test]
fn keys_move_it_only_once_pressed() {
    let Some(mut ui) = session() else { return };
    let m = mount(&mut ui, 40.0, false);
    ui.key(39);
    assert_eq!(m.value.get(), 40.0, "an unfocused gauge took a key");

    press_at(&mut ui, 0.5);
    ui.release().settle();
    assert_eq!(m.value.get(), 50.0);
    assert!(
        !has_attr(&ui, "data-focus-visible="),
        "a press showed the ring"
    );

    ui.key(39);
    assert_eq!(m.value.get(), 51.0);
    assert!(
        has_attr(&ui, "data-focus-visible="),
        "a key did not show the ring"
    );
    ui.key(33);
    assert_eq!(m.value.get(), 61.0);
    ui.key(34).key(40);
    assert_eq!(m.value.get(), 50.0);
    ui.key(35);
    assert_eq!(m.value.get(), 100.0);
    ui.key(36);
    assert_eq!(m.value.get(), 0.0);
    ui.settle().assert_snapshot("cn_number_gauge_focus_visible");

    // A press elsewhere takes focus away.
    ui.move_to(2.0, 2.0).mouse_down().mouse_up();
    ui.key(39);
    assert_eq!(m.value.get(), 0.0, "a blurred gauge took a key");
    assert!(!has_attr(&ui, "data-focus-visible="));
}

#[test]
fn a_value_set_from_code_is_shown_but_not_reported() {
    let Some(mut ui) = session() else { return };
    let m = mount(&mut ui, 40.0, false);
    m.value.set(90.0);
    ui.settle();
    let (g, fill) = (
        bounds(&ui, ".cn-number-gauge"),
        bounds(&ui, ".cn-number-gauge-fill"),
    );
    assert!((fill.width - 0.9 * (g.width - 13.0)).abs() < 1.0);
    assert!(m.changes.lock().unwrap().is_empty());
}

#[test]
fn a_disabled_gauge_does_not_respond() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    let m = mount(&mut ui, 40.0, true);
    assert!(close(
        background(&ui, ".cn-number-gauge-fill"),
        theme.color(ColorToken::InputBgDisabled)
    ));
    assert!(close(
        background(&ui, ".cn-number-gauge-thumb"),
        theme.color(ColorToken::BorderSecondary)
    ));
    ui.assert_snapshot("cn_number_gauge_disabled");
    press_at(&mut ui, 0.9);
    ui.release();
    ui.scroll(0.0, 80.0);
    ui.key(39);
    assert_eq!(m.value.get(), 40.0);
    assert!(m.changes.lock().unwrap().is_empty());
}

#[test]
fn a_bound_disabled_follows_its_signal_in_place() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    let disabled = signal(false);
    let m = mount_with(&mut ui, 40.0, disabled);
    let gauge = ui.find(".cn-number-gauge").unwrap();

    disabled.set(true);
    ui.settle();
    assert_eq!(
        ui.find(".cn-number-gauge"),
        Some(gauge),
        "the gauge was rebuilt"
    );
    assert!(
        has_attr(&ui, "disabled="),
        "the attribute did not follow the signal"
    );
    assert!(close(
        background(&ui, ".cn-number-gauge-fill"),
        theme.color(ColorToken::InputBgDisabled)
    ));
    press_at(&mut ui, 0.9);
    ui.release();
    assert_eq!(m.value.get(), 40.0, "a disabled gauge took a press");

    disabled.set(false);
    ui.settle();
    press_at(&mut ui, 0.9);
    ui.release();
    assert_eq!(m.value.get(), 90.0, "re-enabled, it ignored a press");
}
