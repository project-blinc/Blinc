#![cfg(feature = "headless")]
//! How a cn number range gauge looks and which endpoint the pointer, the wheel
//! and the keyboard move.

use std::sync::{Arc, Mutex};

use blinc_cn::number_range_gauge;
use blinc_core::reactive::{Signal, signal};
use blinc_core::{Brush, Color};
use blinc_layout::div::div;
use blinc_layout::tree::LayoutNodeId;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};
use std::ops::RangeInclusive;

const W: u32 = 220;
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
    value: Signal<RangeInclusive<f64>>,
    changes: Arc<Mutex<Vec<RangeInclusive<f64>>>>,
}

impl Mounted {
    fn ends(&self) -> (f64, f64) {
        let r = self.value.try_get().unwrap();
        (*r.start(), *r.end())
    }

    fn reported(&self) -> Vec<(f64, f64)> {
        let seen = self.changes.lock().unwrap();
        seen.iter().map(|r| (*r.start(), *r.end())).collect()
    }
}

fn mount(ui: &mut Headless, lo: f64, hi: f64) -> Mounted {
    let value = signal(lo..=hi);
    let changes = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&changes);
    ui.mount(move |_| {
        let seen = Arc::clone(&seen);
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(12.0))
            .flex_col()
            .justify_center()
            .child(
                number_range_gauge(value)
                    .w(180.0)
                    .on_change(move |r| seen.lock().unwrap().push(r)),
            )
    });
    ui.leave().settle();
    Mounted { value, changes }
}

/// The upper and lower faces, in paint order.
fn layers(ui: &Headless) -> (LayoutNodeId, LayoutNodeId) {
    let gauge = ui.find(".cn-number-range-gauge").expect("the gauge");
    let kids = ui.tree().layout_tree.children(gauge);
    (kids[0], kids[1])
}

fn fill_of(ui: &Headless, layer: LayoutNodeId) -> Option<Color> {
    let fill = ui.tree().layout_tree.children(layer)[0];
    match &ui.tree().get_render_node(fill).unwrap().props.background {
        Some(Brush::Solid(c)) => Some(*c),
        _ => None,
    }
}

fn close(a: Option<Color>, b: Color) -> bool {
    a.is_some_and(|a| {
        (a.r - b.r).abs() < 0.02 && (a.g - b.g).abs() < 0.02 && (a.b - b.b).abs() < 0.02
    })
}

/// The pointer's x for `value` on the 0 to 100 range.
fn x_of(ui: &Headless, value: f32) -> (f32, f32) {
    let node = ui.find(".cn-number-range-gauge").unwrap();
    let g = ui.tree().get_absolute_bounds(node).unwrap();
    (
        g.x + 1.0 + 5.5 + value / 100.0 * (g.width - 2.0 - 11.0),
        g.y + g.height / 2.0,
    )
}

fn press(ui: &mut Headless, value: f32) {
    let (x, y) = x_of(ui, value);
    ui.move_to(x, y).mouse_down();
}

#[test]
fn the_interval_is_filled_between_the_grips() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount(&mut ui, 20.0, 70.0);
    let (upper, lower) = layers(&ui);
    let primary = theme.color(ColorToken::Primary);
    let input_bg = theme.color(ColorToken::InputBg);
    let tinted = Color::rgba(
        primary.r * 0.22 + input_bg.r * 0.78,
        primary.g * 0.22 + input_bg.g * 0.78,
        primary.b * 0.22 + input_bg.b * 0.78,
        1.0,
    );
    assert!(
        close(fill_of(&ui, upper), tinted),
        "the interval is not tinted"
    );
    assert!(
        close(fill_of(&ui, lower), input_bg),
        "the lower face does not cover the prefix"
    );
    ui.assert_snapshot("cn_number_range_gauge_idle");
}

#[test]
fn a_press_moves_the_nearer_grip_and_they_never_cross() {
    let Some(mut ui) = session() else { return };
    let m = mount(&mut ui, 20.0, 70.0);
    press(&mut ui, 30.0);
    assert_eq!(m.ends(), (30.0, 70.0));
    // Dragging the lower grip past the upper stops at it.
    let (x, y) = x_of(&ui, 95.0);
    ui.move_to(x, y).release().settle();
    assert_eq!(m.ends(), (70.0, 70.0));

    press(&mut ui, 90.0);
    ui.release();
    assert_eq!(m.ends(), (70.0, 90.0));
    assert_eq!(m.reported(), vec![(30.0, 70.0), (70.0, 70.0), (70.0, 90.0)]);
}

#[test]
fn grips_in_one_place_take_turns() {
    let Some(mut ui) = session() else { return };
    let m = mount(&mut ui, 50.0, 50.0);
    // The upper grip is the active one at first, so the first press takes
    // the lower; dragging it right can only reach the upper.
    press(&mut ui, 50.0);
    let (x, y) = x_of(&ui, 30.0);
    ui.move_to(x, y).release().settle();
    assert_eq!(m.ends(), (30.0, 50.0));

    m.value.set(50.0..=50.0);
    ui.settle();
    press(&mut ui, 50.0);
    let (x, y) = x_of(&ui, 80.0);
    ui.move_to(x, y).release().settle();
    assert_eq!(m.ends(), (50.0, 80.0));
}

#[test]
fn the_wheel_and_keys_move_one_endpoint() {
    let Some(mut ui) = session() else { return };
    let m = mount(&mut ui, 20.0, 70.0);
    let (x, y) = x_of(&ui, 25.0);
    ui.move_to(x, y).scroll(0.0, 40.0);
    assert_eq!(m.ends(), (21.0, 70.0));
    let (x, y) = x_of(&ui, 65.0);
    ui.move_to(x, y).scroll(0.0, -40.0);
    assert_eq!(m.ends(), (21.0, 69.0));

    // Keys move the grip last pressed.
    press(&mut ui, 21.0);
    ui.release();
    ui.key(39).key(39);
    assert_eq!(m.ends(), (23.0, 69.0));
    ui.key(35);
    assert_eq!(m.ends(), (69.0, 69.0));
}
