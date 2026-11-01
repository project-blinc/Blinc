#![cfg(feature = "headless")]
//! The loading indicators move by `@keyframes` rules: a spinner's arc turns,
//! its clip turning with it, and a skeleton pulses.

use blinc_cn::{SpinnerSize, skeleton, spinner};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

const S: u32 = 48;

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(S, S).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

#[test]
fn the_arc_turns_a_quarter_in_a_quarter_second() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w(S as f32)
            .h(S as f32)
            .items_center()
            .justify_center()
            .child(spinner().size(SpinnerSize::Large))
    });
    ui.advance(0);
    ui.assert_snapshot("cn_spinner_start");
    ui.advance(250);
    ui.assert_snapshot("cn_spinner_quarter");

    let theme = ThemeState::get();
    let arc = ui.find(".cn-spinner__arc").expect("the arc");
    let track = ui.find(".cn-spinner__track").expect("the track");
    let border = |n| ui.tree().get_render_node(n).unwrap().props.border_color;
    assert_eq!(
        border(arc),
        Some(theme.color(ColorToken::Primary)),
        "arc colour"
    );
    assert_eq!(
        border(track),
        Some(theme.color(ColorToken::Border)),
        "track colour"
    );
}

#[test]
fn a_skeleton_pulses() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w(S as f32)
            .h(S as f32)
            .items_center()
            .justify_center()
            .child(skeleton().w(40.0).h(16.0))
    });
    // The animated opacity is applied as the frame paints, so the frames
    // show it.
    ui.advance(0);
    ui.assert_snapshot("cn_skeleton_start");
    ui.advance(750);
    ui.assert_snapshot("cn_skeleton_half");
}

/// Opening a collapsible grows it over the theme's normal duration rather
/// than at once.
#[test]
fn a_collapsible_opens_over_time() {
    let Some(mut ui) = session() else { return };
    let open = blinc_core::reactive::signal(false);
    ui.mount(move |_| {
        div()
            .w(S as f32)
            .h(S as f32)
            .child(blinc_cn::collapsible(open).content(|| {
                div()
                    .w(S as f32)
                    .h(40.0)
                    .bg(blinc_core::Color::rgb(1.0, 0.0, 0.0))
            }))
    });
    ui.advance(0);
    // How tall the red content is drawn: the rows painted red down the
    // middle column.
    let drawn = |ui: &mut Headless| {
        let frame = ui.snapshot().expect("snapshot");
        let x = frame.width / 2;
        (0..frame.height)
            .filter(|y| {
                let i = ((y * frame.width + x) * 4) as usize;
                let (r, g, b) = (frame.data[i], frame.data[i + 1], frame.data[i + 2]);
                r > 200 && g < 60 && b < 60
            })
            .count() as f32
    };
    assert!(drawn(&mut ui) < 1.0, "starts closed");
    open.set(true);
    ui.advance(50);
    let partway = drawn(&mut ui);
    assert!(
        partway > 1.0 && partway < 39.0,
        "partway open after 50ms: {partway}"
    );
    ui.advance(400);
    let open_rows = drawn(&mut ui);
    assert!((open_rows - 40.0).abs() < 1.5, "open: {open_rows}");
}
