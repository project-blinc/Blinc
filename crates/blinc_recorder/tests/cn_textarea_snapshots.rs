#![cfg(feature = "headless")]
//! How a cn textarea looks idle, hovered, focused, in error and disabled. The
//! border and fill per state are rules, so each state is also checked against
//! the theme token its rule names.

use blinc_cn::textarea;
use blinc_core::Color;
use blinc_layout::div::div;
use blinc_layout::widgets::text_area::text_area_state;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

const W: u32 = 260;
const H: u32 = 140;

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

fn mount(ui: &mut Headless, error: Option<&str>, disabled: bool) {
    blinc_layout::widgets::text_input::blur_all_text_inputs();
    let state = text_area_state();
    let error = error.map(str::to_string);
    ui.mount(move |_| {
        let mut field = textarea(&state)
            .placeholder("Message")
            .w(220.0)
            .rows(3)
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
}

fn border(ui: &Headless) -> Option<Color> {
    let node = ui.find(".cn-textarea").expect("the field");
    ui.tree().get_render_node(node).unwrap().props.border_color
}

fn fill(ui: &Headless) -> Option<Color> {
    let node = ui.find(".cn-textarea").expect("the field");
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
    mount(&mut ui, None, false);
    assert!(close(border(&ui), theme.color(ColorToken::Border)));
    assert!(close(fill(&ui), theme.color(ColorToken::InputBg)));
    ui.assert_snapshot("cn_textarea_idle");

    ui.hover(".cn-textarea").settle();
    assert!(close(border(&ui), theme.color(ColorToken::BorderHover)));
    assert!(close(fill(&ui), theme.color(ColorToken::InputBgHover)));

    ui.click(".cn-textarea").settle();
    assert!(
        close(border(&ui), theme.color(ColorToken::BorderFocus)),
        "focusing did not give the focus border: {:?}",
        border(&ui)
    );
    assert!(close(fill(&ui), theme.color(ColorToken::InputBgFocus)));
    ui.assert_snapshot("cn_textarea_focus");
}

#[test]
fn an_error_keeps_its_border_while_hovered_and_focused() {
    let Some(mut ui) = session() else { return };
    let error = ThemeState::get().color(ColorToken::BorderError);
    mount(&mut ui, Some("Required"), false);
    assert!(close(border(&ui), error));
    ui.assert_snapshot("cn_textarea_error");
    ui.hover(".cn-textarea").settle();
    assert!(close(border(&ui), error), "hover replaced the error border");
    ui.click(".cn-textarea").settle();
    assert!(close(border(&ui), error), "focus replaced the error border");
}

#[test]
fn a_disabled_field_does_not_respond() {
    let Some(mut ui) = session() else { return };
    mount(&mut ui, None, true);
    let idle = border(&ui);
    assert!(close(
        fill(&ui),
        ThemeState::get().color(ColorToken::InputBgDisabled)
    ));
    ui.assert_snapshot("cn_textarea_disabled");
    ui.hover(".cn-textarea").settle();
    assert_eq!(border(&ui), idle, "hover changed a disabled field");
}
