#![cfg(feature = "headless")]
//! How a cn textarea looks idle, hovered, focused, in error and disabled. The
//! border and fill per state are rules, so each state is also checked against
//! the theme token its rule names.

use blinc_cn::textarea;
use blinc_core::Color;
use blinc_layout::div::div;
use blinc_layout::text_measure::TextFace;
use blinc_layout::widgets::text_area::{
    SharedTextAreaState, TextAreaConfig, text_area, text_area_state,
};
use blinc_recorder::testing::Golden;
use blinc_recorder::{CapturedFrame, Headless, ScreenshotExporter};
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
    // The caret blinks on the wall clock, so a focused frame differs a little
    // from run to run.
    let frame = ui.snapshot().expect("snapshot");
    Golden::in_crate()
        .tolerance(24)
        .check("cn_textarea_focus", &frame);
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

#[test]
fn only_a_focused_caret_asks_for_frames() {
    let Some(mut ui) = session() else { return };
    mount(&mut ui, None, false);
    ui.snapshot().expect("snapshot");
    assert!(
        !ui.tree().canvas_wants_frames(),
        "an unfocused textarea kept its caret asking for frames"
    );

    ui.click(".cn-textarea").settle();
    ui.snapshot().expect("snapshot");
    assert!(
        ui.tree().canvas_wants_frames(),
        "a focused caret stopped asking for frames to blink"
    );

    blinc_layout::widgets::text_input::blur_all_text_inputs();
    ui.settle().snapshot().expect("snapshot");
    assert!(
        !ui.tree().canvas_wants_frames(),
        "a blurred textarea kept its caret asking for frames"
    );
}

fn save_interaction(frame: &CapturedFrame, name: &str) {
    if let Some(directory) = std::env::var_os("BLINC_TEXT_AREA_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        ScreenshotExporter::save_png(frame, directory.join(format!("{name}.png"))).unwrap();
    }
}

#[test]
fn double_click_paints_the_selected_word() {
    let Some(mut ui) = session() else { return };
    let state = text_area_state();
    state.lock().unwrap().set_value("hello world\nsecond line");
    let value = state.clone();
    ui.mount(move |_| {
        div()
            .size(W as f32, H as f32)
            .p(3.0)
            .child(text_area(&value).id("notes").w(220.0).rows(3))
    });
    ui.settle().snapshot().expect("warm frame");
    let cfg = TextAreaConfig::default();
    let node = ui.find("#notes").unwrap();
    let bounds = ui.tree().get_absolute_bounds(node).unwrap();
    let x = bounds.x
        + cfg.padding_x
        + cfg.border_width
        + TextFace::default().measure("hello w", cfg.font_size).width;
    let y = bounds.y + cfg.padding_y + cfg.border_width + cfg.font_size * 0.5;
    ui.key(9).settle();
    assert_eq!(ui.focused(), Some(node));
    let before = ui.snapshot().expect("focused without selection");
    // Capture outside the click pair so GPU readback cannot expire the double-click window.
    ui.move_to(x, y)
        .mouse_down()
        .mouse_up()
        .mouse_down()
        .mouse_up();
    let selected = ui.snapshot().expect("double click");
    assert_eq!(
        state.lock().unwrap().selected_text().as_deref(),
        Some("world")
    );
    assert!(
        before.diff_pixel_count(&selected) > 100,
        "selection was not painted"
    );
    save_interaction(&selected, "word-selection");
}

fn mount_growing(ui: &mut Headless, state: &SharedTextAreaState) {
    let rows = state.lock().unwrap().line_count();
    let state = state.clone();
    ui.mount(move |_| {
        div()
            .size(W as f32, H as f32)
            .p(3.0)
            .child(text_area(&state).id("notes").w(220.0).rows(rows))
    });
    ui.settle();
}

#[test]
fn growing_rows_keep_the_first_line_visible() {
    let Some(mut ui) = session() else { return };
    let state = text_area_state();
    state.lock().unwrap().set_value("First line");
    mount_growing(&mut ui, &state);
    ui.click("#notes").key(35);
    for _ in 0..3 {
        ui.key(13).type_text("Next line");
        mount_growing(&mut ui, &state);
        assert!(state.lock().unwrap().scroll_offset().abs() < 0.01);
    }
    let frame = ui.snapshot().expect("grown field");
    let bounds = ui
        .tree()
        .get_absolute_bounds(ui.find("#notes").unwrap())
        .unwrap();
    let cfg = TextAreaConfig::default();
    let top = (bounds.y + cfg.padding_y + cfg.border_width) as u32;
    let left = (bounds.x + cfg.padding_x + cfg.border_width) as u32;
    let bottom = top + (cfg.font_size * cfg.line_height) as u32;
    let ink = (top..bottom)
        .flat_map(|y| (left..left + 90).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            frame
                .get_pixel(x, y)
                .is_some_and(|p| p[..3].iter().all(|&v| v > 160))
        })
        .count();
    assert!(
        ink > 20,
        "first-line glyphs were clipped: {ink} bright pixels"
    );
    save_interaction(&frame, "grown-rows");
}
