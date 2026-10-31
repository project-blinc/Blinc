#![cfg(feature = "headless")]
//! How a cn select trigger and a dropdown menu's default trigger look idle,
//! hovered, pressed, open and disabled. Their fills and borders are rules, so
//! each state is checked against the token or tint its rule names.

use blinc_cn::{combobox, dropdown_menu, select};
use blinc_core::Color;
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

const W: u32 = 260;
const H: u32 = 120;

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

fn mount_select(ui: &mut Headless, disabled: bool) {
    let value = State::new(
        signal::<String>(String::new()),
        global_graph(),
        global_dirty_flag(),
    );
    ui.mount(move |_| {
        div().w(W as f32).h(H as f32).p(4.0).child(
            select(&value)
                .option("a", "Apple")
                .option("b", "Banana")
                .placeholder("Fruit")
                .w(180.0)
                .disabled(disabled),
        )
    });
    ui.leave().settle();
}

fn props(ui: &Headless, class: &str) -> (Option<Color>, Option<Color>) {
    let node = ui.find(class).expect("the trigger");
    let p = &ui.tree().get_render_node(node).unwrap().props;
    let fill = match &p.background {
        Some(blinc_core::Brush::Solid(c)) => Some(*c),
        _ => None,
    };
    (fill, p.border_color)
}

fn same(a: Option<Color>, b: Color) -> bool {
    a.is_some_and(|a| {
        (a.r - b.r).abs() < 0.02 && (a.g - b.g).abs() < 0.02 && (a.b - b.b).abs() < 0.02
    })
}

fn alpha(a: Option<Color>) -> f32 {
    a.map_or(-1.0, |c| c.a)
}

#[test]
fn the_select_trigger_follows_its_rules() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount_select(&mut ui, false);
    let (fill, border) = props(&ui, ".cn-select-trigger");
    assert!(alpha(fill) < 0.01, "idle fill is not clear: {fill:?}");
    assert!(same(border, theme.color(ColorToken::Border)));
    ui.assert_snapshot("cn_select_idle");

    ui.hover(".cn-select-trigger").settle();
    let (fill, _) = props(&ui, ".cn-select-trigger");
    assert!((alpha(fill) - 0.05).abs() < 0.02, "hover tint: {fill:?}");

    ui.press(".cn-select-trigger").settle();
    let (fill, _) = props(&ui, ".cn-select-trigger");
    assert!((alpha(fill) - 0.10).abs() < 0.02, "press tint: {fill:?}");
    ui.release().settle();

    let (_, border) = props(&ui, ".cn-select-trigger");
    assert!(
        same(border, theme.color(ColorToken::BorderHover)),
        "an open select did not darken its border: {border:?}"
    );
    ui.assert_snapshot("cn_select_open");
}

#[test]
fn a_disabled_select_does_not_respond() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount_select(&mut ui, true);
    let (fill, border) = props(&ui, ".cn-select-trigger");
    assert!(same(fill, theme.color(ColorToken::InputBgDisabled)));
    assert!(same(border, theme.color(ColorToken::BorderSecondary)));
    ui.assert_snapshot("cn_select_disabled");
    ui.hover(".cn-select-trigger").settle();
    assert_eq!(props(&ui, ".cn-select-trigger"), (fill, border));
}

#[test]
fn the_dropdown_trigger_tints_on_hover_and_press() {
    let Some(mut ui) = session() else { return };
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .p(4.0)
            .child(dropdown_menu("Actions").item("Edit", || {}))
    });
    ui.leave().settle();
    let (fill, _) = props(&ui, ".cn-dropdown-trigger");
    assert!(alpha(fill) < 0.01, "idle fill is not clear: {fill:?}");
    ui.assert_snapshot("cn_dropdown_trigger_idle");

    ui.hover(".cn-dropdown-trigger").settle();
    let (fill, _) = props(&ui, ".cn-dropdown-trigger");
    assert!((alpha(fill) - 0.05).abs() < 0.02, "hover tint: {fill:?}");

    ui.press(".cn-dropdown-trigger").settle();
    let (fill, _) = props(&ui, ".cn-dropdown-trigger");
    assert!((alpha(fill) - 0.10).abs() < 0.02, "press tint: {fill:?}");
}

fn mount_combobox(ui: &mut Headless, disabled: bool) {
    let value = State::new(
        signal::<String>(String::new()),
        global_graph(),
        global_dirty_flag(),
    );
    ui.mount(move |_| {
        div().w(W as f32).h(H as f32).p(4.0).child(
            combobox(&value)
                .option("a", "Apple")
                .option("b", "Banana")
                .placeholder("Fruit")
                .w(180.0)
                .disabled(disabled),
        )
    });
    ui.leave().settle();
}

#[test]
fn the_combobox_trigger_follows_its_rules() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount_combobox(&mut ui, false);
    let (fill, border) = props(&ui, ".cn-combobox-trigger");
    assert!(same(fill, theme.color(ColorToken::Surface)));
    assert!(same(border, theme.color(ColorToken::Border)));
    ui.assert_snapshot("cn_combobox_idle");

    ui.hover(".cn-combobox-trigger").settle();
    let (_, border) = props(&ui, ".cn-combobox-trigger");
    assert!(same(border, theme.color(ColorToken::BorderHover)));

    ui.click(".cn-combobox-trigger").settle();
    assert!(
        ui.find(".cn-combobox-content").is_some(),
        "the list did not open"
    );
    let (_, border) = props(&ui, ".cn-combobox-trigger");
    assert!(
        same(border, theme.color(ColorToken::BorderFocus)),
        "an open combobox did not take the focus border: {border:?} (hover {:?}, focus {:?}, idle {:?})",
        theme.color(ColorToken::BorderHover),
        theme.color(ColorToken::BorderFocus),
        theme.color(ColorToken::Border)
    );
}

#[test]
fn a_disabled_combobox_does_not_respond() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount_combobox(&mut ui, true);
    let (fill, border) = props(&ui, ".cn-combobox-trigger");
    assert!(same(fill, theme.color(ColorToken::InputBgDisabled)));
    assert!(same(border, theme.color(ColorToken::BorderSecondary)));
    ui.hover(".cn-combobox-trigger").settle();
    assert_eq!(props(&ui, ".cn-combobox-trigger"), (fill, border));
}
