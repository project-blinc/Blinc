#![cfg(feature = "headless")]
//! How a cn slider looks and answers the pointer, the wheel and the keyboard.
//! Its colours are rules, so each state is checked against the theme tokens
//! its rule names as well as by snapshot.

use std::sync::{Arc, Mutex};

use blinc_cn::slider;
use blinc_core::reactive::{Signal, signal};
use blinc_core::{Brush, Color};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

const W: u32 = 240;
const H: u32 = 72;
/// The medium thumb.
const THUMB: f32 = 16.0;

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
    value: Signal<f32>,
    changes: Arc<Mutex<Vec<f32>>>,
}

fn mount(ui: &mut Headless, disabled: Signal<bool>) -> Mounted {
    let changes = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&changes);
    let value = signal(40.0f32);
    ui.mount(move |_| {
        let seen = Arc::clone(&seen);
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(12.0))
            .flex_col()
            .justify_center()
            .child(
                slider(value)
                    .max(100.0)
                    .step(1.0)
                    .w(200.0)
                    .label("Volume")
                    .show_value()
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

fn close(a: Option<Color>, b: Color) -> bool {
    a.is_some_and(|a| {
        (a.r - b.r).abs() < 0.02 && (a.g - b.g).abs() < 0.02 && (a.b - b.b).abs() < 0.02
    })
}

/// The point on the track whose value is `fraction` of the range.
fn along(ui: &Headless, fraction: f32) -> (f32, f32) {
    let s = bounds(ui, ".cn-slider");
    (
        s.x + THUMB / 2.0 + fraction * (s.width - THUMB),
        s.y + s.height / 2.0,
    )
}

fn has_attr(ui: &Headless, attr: &str) -> bool {
    let node = ui.find(".cn-slider").expect("the slider");
    ui.tree()
        .element_registry()
        .get_classes(node)
        .unwrap_or_default()
        .iter()
        .any(|c| &**c == attr)
}

#[test]
fn the_parts_follow_their_rules() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount(&mut ui, signal(false));
    assert!(close(
        background(&ui, ".cn-slider-fill"),
        theme.color(ColorToken::Primary)
    ));
    assert!(close(
        background(&ui, ".cn-slider-rest"),
        theme.color(ColorToken::Border)
    ));
    assert!(close(
        background(&ui, ".cn-slider-thumb"),
        theme.color(ColorToken::TextInverse)
    ));
    // The thumb sits at its value along the travel.
    let (s, thumb) = (bounds(&ui, ".cn-slider"), bounds(&ui, ".cn-slider-thumb"));
    assert!((thumb.x - s.x - 0.4 * (s.width - THUMB)).abs() < 1.0);
    ui.assert_snapshot("cn_slider_idle");

    ui.hover(".cn-slider").settle();
    ui.assert_snapshot("cn_slider_hover");
}

#[test]
fn a_press_on_the_track_seeks_and_a_drag_follows() {
    let Some(mut ui) = session() else { return };
    let m = mount(&mut ui, signal(false));
    let (x, y) = along(&ui, 0.75);
    ui.move_to(x, y).mouse_down();
    assert_eq!(m.value.get(), 75.0);
    assert!(
        has_attr(&ui, "data-state=dragging"),
        "not dragging while held"
    );

    let s = bounds(&ui, ".cn-slider");
    ui.move_to(s.x + s.width + 40.0, y + 30.0);
    assert_eq!(m.value.get(), 100.0, "a drag past the end stops at max");
    ui.release().settle();
    assert!(!has_attr(&ui, "data-state=dragging"));
    assert_eq!(*m.changes.lock().unwrap(), vec![75.0, 100.0]);
}

#[test]
fn a_press_on_the_thumb_holds_it_where_it_was_grabbed() {
    let Some(mut ui) = session() else { return };
    let m = mount(&mut ui, signal(false));
    let (x, y) = along(&ui, 0.4);
    ui.move_to(x + 5.0, y).mouse_down();
    assert_eq!(m.value.get(), 40.0, "the press moved the thumb");
    let travel = bounds(&ui, ".cn-slider").width - THUMB;
    ui.move_to(x + 5.0 + 0.1 * travel, y);
    assert_eq!(m.value.get(), 50.0);
    ui.release();
}

#[test]
fn the_wheel_steps_the_value() {
    let Some(mut ui) = session() else { return };
    let m = mount(&mut ui, signal(false));
    ui.hover(".cn-slider");
    ui.scroll(0.0, 40.0);
    assert_eq!(m.value.get(), 41.0);
    ui.scroll(0.0, -40.0);
    assert_eq!(m.value.get(), 40.0);
}

#[test]
fn keys_move_it_only_once_pressed() {
    let Some(mut ui) = session() else { return };
    let m = mount(&mut ui, signal(false));
    ui.key(39);
    assert_eq!(m.value.get(), 40.0, "an unfocused slider took a key");

    let (x, y) = along(&ui, 0.5);
    ui.move_to(x, y).mouse_down();
    ui.release().settle();
    assert_eq!(m.value.get(), 50.0);
    ui.key(39);
    assert_eq!(m.value.get(), 51.0);
    assert!(
        has_attr(&ui, "data-focus-visible="),
        "a key did not show the ring"
    );
    ui.key(33);
    assert_eq!(m.value.get(), 61.0);
    ui.key(35);
    assert_eq!(m.value.get(), 100.0);
    ui.key(36);
    assert_eq!(m.value.get(), 0.0);
}

#[test]
fn a_bound_disabled_mutes_it_in_place() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    let disabled = signal(false);
    let m = mount(&mut ui, disabled);
    let thumb = ui.find(".cn-slider-thumb").unwrap();

    disabled.set(true);
    ui.settle();
    assert_eq!(
        ui.find(".cn-slider-thumb"),
        Some(thumb),
        "the slider was rebuilt"
    );
    assert!(close(
        background(&ui, ".cn-slider-rest"),
        theme.color(ColorToken::InputBgDisabled)
    ));
    assert!(close(
        background(&ui, ".cn-slider-fill"),
        theme.color(ColorToken::BorderSecondary)
    ));
    ui.assert_snapshot("cn_slider_disabled");
    let (x, y) = along(&ui, 0.9);
    ui.move_to(x, y).mouse_down();
    ui.release();
    ui.scroll(0.0, 80.0);
    assert_eq!(m.value.get(), 40.0, "a disabled slider moved");

    disabled.set(false);
    ui.settle();
    ui.move_to(x, y).mouse_down();
    ui.release();
    assert_eq!(m.value.get(), 90.0, "enabled again, it did not move");
}
