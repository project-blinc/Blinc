use super::super::Headless;
use blinc_core::{Color, Transform, reactive::signal};
use blinc_layout::div::div;

#[test]
fn cached_paint_follows_a_reactive_transform() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(128, 80).unwrap();
    ui.theme(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Dark,
    );
    let offset = signal(0.0f32);
    ui.mount(move |_| {
        div().w(128.0).h(80.0).bg(Color::BLACK).child(
            div()
                .w(24.0)
                .h(24.0)
                .bg(Color::RED)
                .bind_transform_from(offset, |x| Transform::translate(x, 0.0)),
        )
    });
    let before = ui.snapshot().unwrap();
    offset.set(48.0);
    let cached = ui.snapshot_fast().unwrap();
    let fresh = ui.snapshot().unwrap();
    assert!(
        fresh.data != before.data,
        "the reference transform must move"
    );
    assert!(
        cached.data == fresh.data,
        "cached paint must show the current property value"
    );
}
