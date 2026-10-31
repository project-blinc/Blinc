#![cfg(feature = "headless")]
//! How a cn input looks idle, hovered, focused, in error and disabled. The
//! border and fill per state are rules, so each state is also checked against
//! the theme token its rule names.

use blinc_cn::input;
use blinc_core::Color;
use blinc_layout::div::div;
use blinc_layout::widgets::text_input::{SharedTextInputData, text_input_data};
use blinc_recorder::Headless;
use blinc_recorder::testing::Golden;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

const W: u32 = 240;
const H: u32 = 72;

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

fn mount(
    ui: &mut Headless,
    value: &str,
    error: Option<&str>,
    disabled: bool,
) -> SharedTextInputData {
    blinc_layout::widgets::text_input::blur_all_text_inputs();
    let data = text_input_data();
    data.lock().unwrap().value = value.to_string();
    let bound = data.clone();
    let error = error.map(str::to_string);
    ui.mount(move |_| {
        let mut field = input(&bound)
            .placeholder("Email")
            .w(200.0)
            .disabled(disabled);
        if let Some(error) = &error {
            field = field.error(error.clone());
        }
        div()
            .w(W as f32)
            .h(H as f32)
            .items_center()
            .justify_center()
            .child(field)
    });
    ui.leave().settle();
    data
}

fn border(ui: &Headless) -> Option<Color> {
    let node = ui.find(".cn-input").expect("the field");
    ui.tree().get_render_node(node).unwrap().props.border_color
}

fn fill(ui: &Headless) -> Option<Color> {
    let node = ui.find(".cn-input").expect("the field");
    match &ui.tree().get_render_node(node).unwrap().props.background {
        Some(blinc_core::Brush::Solid(c)) => Some(*c),
        _ => None,
    }
}

fn close(a: Option<Color>, b: Color) -> bool {
    a.is_some_and(|a| {
        (a.r - b.r).abs() < 0.02 && (a.g - b.g).abs() < 0.02 && (a.b - b.b).abs() < 0.02
    })
}

#[test]
fn idle_hover_and_focus_follow_their_rules() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount(&mut ui, "", None, false);
    assert!(close(border(&ui), theme.color(ColorToken::Border)));
    assert!(close(fill(&ui), theme.color(ColorToken::InputBg)));
    ui.assert_snapshot("cn_input_idle");

    ui.hover(".cn-input").settle();
    assert!(close(border(&ui), theme.color(ColorToken::BorderHover)));
    assert!(close(fill(&ui), theme.color(ColorToken::InputBgHover)));
    ui.assert_snapshot("cn_input_hover");

    ui.click(".cn-input").settle();
    assert!(
        close(border(&ui), theme.color(ColorToken::BorderFocus)),
        "focusing did not give the focus border: {:?}",
        border(&ui)
    );
    assert!(close(fill(&ui), theme.color(ColorToken::InputBgFocus)));
    // The caret blinks on the wall clock, so a focused frame differs a little
    // from run to run.
    let frame = ui.snapshot().expect("snapshot");
    Golden::in_crate()
        .tolerance(24)
        .check("cn_input_focus", &frame);

    ui.leave().settle();
    assert!(
        close(border(&ui), theme.color(ColorToken::BorderFocus)),
        "leaving a focused field dropped its focus border"
    );
}

#[test]
fn an_error_keeps_its_border_while_hovered_and_focused() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount(&mut ui, "", Some("Required"), false);
    let error = theme.color(ColorToken::BorderError);
    assert!(close(border(&ui), error));
    ui.assert_snapshot("cn_input_error");
    ui.hover(".cn-input").settle();
    assert!(close(border(&ui), error), "hover replaced the error border");
    ui.click(".cn-input").settle();
    assert!(close(border(&ui), error), "focus replaced the error border");
}

#[test]
fn an_invalid_value_shows_the_error_border_without_a_message() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    let data = mount(&mut ui, "nope", None, false);
    assert!(!close(border(&ui), theme.color(ColorToken::BorderError)));
    data.lock().unwrap().is_valid = false;
    blinc_layout::widgets::text_input::refresh_text_input(&data);
    ui.settle();
    assert!(close(border(&ui), theme.color(ColorToken::BorderError)));
}

#[test]
fn a_disabled_field_does_not_respond() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount(&mut ui, "", None, true);
    let idle = border(&ui);
    assert!(close(fill(&ui), theme.color(ColorToken::InputBgDisabled)));
    ui.assert_snapshot("cn_input_disabled");
    ui.hover(".cn-input").settle();
    assert_eq!(border(&ui), idle, "hover changed a disabled field");
}
