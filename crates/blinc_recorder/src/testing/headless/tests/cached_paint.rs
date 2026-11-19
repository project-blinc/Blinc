use super::super::Headless;
use blinc_core::{
    Color, Transform,
    reactive::{computed, signal},
};
use blinc_layout::div::div;
use blinc_layout::text::text;

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
    offset.set(16.0);
    ui.snapshot_fast().unwrap();
    offset.set(48.0);
    ui.apply_queued_updates();
    assert!(
        ui.app.has_render_cache(),
        "a primed translation must retain its paint"
    );
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

#[test]
fn cached_computed_translation_keeps_text_and_clips_aligned() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(160, 80).unwrap();
    ui.theme(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Dark,
    );
    let offset = signal(0.0f32);
    let translated = computed(move |g| Transform::translate(g.get(offset).unwrap(), 0.0));
    ui.mount(move |_| {
        div().w(160.0).h(80.0).bg(Color::BLACK).child(
            div().w(88.0).h(60.0).overflow_clip().child(
                div()
                    .w(80.0)
                    .h(40.0)
                    .bg(Color::RED)
                    .transform(translated.clone())
                    .child(text("Calendar").color(Color::WHITE)),
            ),
        )
    });
    ui.snapshot().unwrap();
    offset.set(8.0);
    ui.snapshot_fast().unwrap();
    for x in [24.0, 24.25, 40.0, 0.0, -16.0] {
        offset.set(x);
        ui.apply_queued_updates();
        assert!(
            ui.app.has_render_cache(),
            "computed translation discarded paint"
        );
        let cached = ui.snapshot_fast().unwrap();
        let fresh = ui.snapshot().unwrap();
        assert!(cached.data == fresh.data, "text or clip drift at {x}");
    }
}

#[test]
fn cached_transform_falls_back_for_scale_and_other_property_writes() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(128, 80).unwrap();
    ui.theme(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Dark,
    );
    let transform = signal(Transform::translate(0.0, 0.0));
    let color = signal(Color::RED);
    ui.mount(move |_| {
        div()
            .w(128.0)
            .h(80.0)
            .bg(Color::BLACK)
            .child(div().w(24.0).h(24.0).transform(transform).bg(color))
    });
    ui.snapshot().unwrap();
    transform.set(Transform::translate(8.0, 0.0));
    ui.snapshot_fast().unwrap();
    transform.set(Transform::scale(2.0, 2.0));
    ui.apply_queued_updates();
    assert!(!ui.app.has_render_cache(), "scale must repaint");
    let cached = ui.snapshot_fast().unwrap();
    assert!(cached.data == ui.snapshot().unwrap().data);
    transform.set(Transform::translate(16.0, 0.0));
    ui.snapshot_fast().unwrap();
    transform.set(Transform::translate(32.0, 0.0));
    color.set(Color::WHITE);
    ui.apply_queued_updates();
    assert!(
        !ui.app.has_render_cache(),
        "a simultaneous color write must repaint"
    );
    let cached = ui.snapshot_fast().unwrap();
    assert!(cached.data == ui.snapshot().unwrap().data);
}

#[test]
fn cached_translation_repaints_under_a_scaled_ancestor() {
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
                .w(48.0)
                .h(40.0)
                .transform(Transform::scale(1.5, 1.5))
                .child(
                    div()
                        .w(24.0)
                        .h(24.0)
                        .bg(Color::RED)
                        .bind_transform_from(offset, |x| Transform::translate(x, 0.0)),
                ),
        )
    });
    ui.snapshot().unwrap();
    for x in [8.0, 24.0] {
        offset.set(x);
        ui.apply_queued_updates();
        assert!(!ui.app.has_render_cache(), "scaled ancestry must repaint");
        let cached = ui.snapshot_fast().unwrap();
        assert!(cached.data == ui.snapshot().unwrap().data);
    }
}

#[test]
fn cached_nested_reactive_translations_accumulate_once() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(160, 80).unwrap();
    ui.theme(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Dark,
    );
    let parent = signal(0.0f32);
    let child = signal(0.0f32);
    ui.mount(move |_| {
        div().w(160.0).h(80.0).bg(Color::BLACK).child(
            div()
                .w(96.0)
                .h(60.0)
                .bind_transform_from(parent, |x| Transform::translate(x, 0.0))
                .child(
                    div()
                        .w(24.0)
                        .h(24.0)
                        .bg(Color::RED)
                        .bind_transform_from(child, |y| Transform::translate(0.0, y)),
                ),
        )
    });
    ui.snapshot().unwrap();
    parent.set(8.0);
    child.set(4.0);
    ui.snapshot_fast().unwrap();
    for (x, y) in [(24.0, 12.0), (0.0, 0.0), (40.0, 24.0)] {
        parent.set(x);
        child.set(y);
        ui.apply_queued_updates();
        assert!(
            ui.app.has_render_cache(),
            "nested translation discarded paint"
        );
        let cached = ui.snapshot_fast().unwrap();
        assert!(cached.data == ui.snapshot().unwrap().data);
    }
}

#[test]
fn cached_reactive_translation_repaints_with_motion_promotion() {
    if !Headless::available() {
        return;
    }
    let mut ui =
        Headless::with_motion_paint(128, 80, super::super::super::MotionPaint::Promoted).unwrap();
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
    ui.snapshot().unwrap();
    for x in [8.0, 24.0] {
        offset.set(x);
        ui.apply_queued_updates();
        assert!(
            !ui.app.has_render_cache(),
            "promoted mode must repaint property transforms"
        );
        let cached = ui.snapshot_fast().unwrap();
        assert!(cached.data == ui.snapshot().unwrap().data);
    }
}

#[test]
fn cached_translation_keeps_an_inner_viewport_clip_inside_a_scroll_view() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::with_scale(160, 100, 2.0).unwrap();
    ui.theme(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Dark,
    );
    let offset = signal(0.0f32);
    ui.mount(move |_| {
        blinc_layout::widgets::scroll::scroll()
            .w(160.0)
            .h(100.0)
            .bg(Color::BLACK)
            .child(
                div()
                    .w(160.0)
                    .child(
                        div().w(88.0).h(60.0).overflow_clip().ml(6.0).mt(6.0).child(
                            div()
                                .w(80.0)
                                .h(40.0)
                                .bg(Color::RED)
                                .id("moving")
                                .bind_transform_from(offset, |x| Transform::translate(x, 0.0))
                                .child(text("Calendar").color(Color::WHITE)),
                        ),
                    )
                    .child(div().w(1.0).h(80.0)),
            )
    });
    ui.snapshot().unwrap();
    ui.move_to(150.0, 90.0).scroll(0.0, -8.0);
    ui.snapshot_fast().unwrap();
    offset.set(8.0);
    ui.snapshot_fast().unwrap();
    let moving = ui.find("#moving").unwrap();
    assert!(
        ui.tree()
            .composite_bindings()
            .get(&moving)
            .unwrap()
            .dynamic_batch
    );
    offset.set(-16.0);
    ui.apply_queued_updates();
    assert!(ui.app.has_render_cache());
    let cached = ui.snapshot_fast().unwrap();
    let fresh = ui.snapshot().unwrap();
    if cached.data != fresh.data
        && let Ok(prefix) = std::env::var("BLINC_CACHED_PAINT_DEBUG")
    {
        crate::ScreenshotExporter::save_png(&cached, format!("{prefix}-cached.png")).unwrap();
        crate::ScreenshotExporter::save_png(&fresh, format!("{prefix}-full.png")).unwrap();
    }
    assert!(cached.data == fresh.data, "text escaped the inner viewport");
}

#[test]
fn explicit_repaint_survives_queued_translation_writes() {
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
    ui.snapshot().unwrap();
    offset.set(8.0);
    ui.snapshot_fast().unwrap();
    blinc_layout::request_redraw();
    ui.apply_queued_updates();
    assert!(
        !ui.app.has_render_cache(),
        "an explicit repaint must discard paint"
    );
    ui.snapshot_fast().unwrap();
    for (i, explicit_first) in [true, false].into_iter().enumerate() {
        if explicit_first {
            blinc_layout::request_redraw();
        }
        offset.set(24.0 + i as f32 * 16.0);
        if !explicit_first {
            blinc_layout::request_redraw();
        }
        ui.apply_queued_updates();
        assert!(
            !ui.app.has_render_cache(),
            "property writes must preserve an explicit repaint"
        );
        let cached = ui.snapshot_fast().unwrap();
        assert!(cached.data == ui.snapshot().unwrap().data);
    }
}
