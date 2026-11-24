use super::*;
use blinc_core::{Brush, Color};
use blinc_theme::{ColorToken, ThemeState};

fn dot_color(ui: &mut Headless) -> Color {
    let dot = all(ui, "cn-calendar-event")[0];
    let bounds = ui.tree().get_absolute_bounds(dot).unwrap();
    let color = match ui
        .tree()
        .get_render_node(dot)
        .unwrap()
        .props
        .background
        .as_ref()
        .unwrap()
    {
        Brush::Solid(color) => *color,
        _ => panic!("solid dot"),
    };
    let pixel = ui
        .snapshot()
        .unwrap()
        .get_pixel(
            (bounds.x + bounds.width / 2.0) as u32,
            (bounds.y + bounds.height / 2.0) as u32,
        )
        .unwrap();
    let expected = [
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
    ];
    assert!(
        (0..3).all(|c| pixel[c].abs_diff(expected[c]) <= 3),
        "dot paint {pixel:?} vs {expected:?}"
    );
    color
}

fn check_selected_events(initially_selected: bool, scheme: ColorScheme) {
    let Some(mut ui) = session() else { return };
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        scheme,
    );
    let picked = signal(initially_selected.then(|| day(12)));
    let events = signal(vec![CalendarEvent::new(day(12), "Ship").kind("deadline")]);
    let inverse = ThemeState::get().color(ColorToken::TextInverse);
    let red = Color::from_hex(0xef4444);
    ui.context()
        .add_css(r#"#showcase .cn-calendar-event[data-kind="deadline"] { background: #ef4444; }"#);
    ui.mount(move |_| {
        div()
            .id("showcase")
            .w(W as f32)
            .h(H as f32)
            .child(blinc_cn::calendar(picked).today(day(1)).events(events))
    });
    ui.leave().settle();
    assert_eq!(
        dot_color(&mut ui),
        if initially_selected { inverse } else { red }
    );
    picked.set(Some(day(12)));
    ui.settle();
    assert_eq!(dot_color(&mut ui), inverse, "selected dots retain contrast");
    picked.set(None);
    ui.settle();
    assert_eq!(dot_color(&mut ui), red, "deselection restores app colour");
}

#[test]
fn selected_events_keep_contrast_and_restore_app_colour_in_dark() {
    check_selected_events(false, ColorScheme::Dark);
}

#[test]
fn initially_selected_events_restore_app_colour_in_dark() {
    check_selected_events(true, ColorScheme::Dark);
}

#[test]
fn selected_events_keep_contrast_and_restore_app_colour_in_light() {
    check_selected_events(false, ColorScheme::Light);
}

#[test]
fn initially_selected_events_restore_app_colour_in_light() {
    check_selected_events(true, ColorScheme::Light);
}
