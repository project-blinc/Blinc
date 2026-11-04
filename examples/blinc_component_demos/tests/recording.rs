#![cfg(feature = "capture")]
use blinc_component_demos::{
    DEMO_CSS, FONTS,
    demos::{DemoState, build},
    theme_bundle,
};
use blinc_recorder::Headless;
use blinc_theme::ColorScheme;

#[path = "../src/cursor.rs"]
mod cursor;

#[test]
fn css_edits_replace_rules_and_root_tokens_without_losing_values() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(560, 320).expect("offscreen GPU");
    ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
    for font in FONTS {
        ui.load_font_data(font.to_vec());
    }
    let values = DemoState::default();
    let fixture = values.clone();
    ui.mount(move |ctx| build("button", &fixture, (ctx.width, ctx.height)))
        .settle();
    ui.click("#demo-action").move_to(0.0, 0.0).settle();
    let sample = |ui: &mut Headless| {
        let node = ui.find("#demo-action").unwrap();
        let bounds = ui.tree().get_absolute_bounds(node).unwrap();
        ui.snapshot()
            .unwrap()
            .get_pixel(
                (bounds.x + 6.0) as u32,
                (bounds.y + bounds.height / 2.0) as u32,
            )
            .unwrap()
    };
    let default = sample(&mut ui);
    ui.context()
        .set_css("preview", ":root { --primary: #e0b000; }")
        .unwrap();
    ui.settle();
    let custom = sample(&mut ui);
    assert_ne!(
        custom, default,
        "root variable should recolour the existing component"
    );
    assert_eq!(values.clicks.get(), 1);
    assert!(
        ui.context()
            .set_css("preview", ".cn-button { background:")
            .is_err()
    );
    ui.settle();
    assert_eq!(
        sample(&mut ui),
        custom,
        "invalid CSS preserves the previous styles"
    );
    ui.context().set_css("preview", "").unwrap();
    ui.settle();
    assert_eq!(
        sample(&mut ui),
        default,
        "removed root variables restore the theme"
    );
    assert_eq!(values.clicks.get(), 1);
    for recipe in ["button", "input", "select", "dialog", "progress", "charts"] {
        let css =
            std::fs::read_to_string(format!("../../web/components/recipes/{recipe}.css")).unwrap();
        ui.context()
            .set_css("preview", &css)
            .unwrap_or_else(|error| panic!("{recipe}: {error}"));
        ui.settle();
    }
}

#[test]
fn density_preserves_layout_and_pointer_hit_testing() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::with_scale(560, 320, 2.0).expect("offscreen GPU");
    ui.theme(theme_bundle(), ColorScheme::Light).css(DEMO_CSS);
    for font in FONTS {
        ui.load_font_data(font.to_vec());
    }
    let state = DemoState::default();
    let fixture = state.clone();
    ui.mount(move |ctx| build("button", &fixture, (ctx.width, ctx.height)))
        .settle();
    assert_eq!(ui.context().width, 560.0);
    assert_eq!(ui.context().height, 320.0);
    assert_eq!(ui.context().physical_width(), 1120.0);
    assert_eq!(ui.context().physical_height(), 640.0);
    let node = ui.find("#demo-action").unwrap();
    let bounds = ui.tree().get_absolute_bounds(node).unwrap();
    assert!(bounds.x > 0.0 && bounds.x + bounds.width < 560.0);
    ui.click("#demo-action").settle();
    assert_eq!(state.clicks.get(), 1);
    let image = ui.snapshot().unwrap();
    assert_eq!((image.width, image.height), (1120, 640));
}

#[test]
fn recording_cursor_is_above_dialogs_and_does_not_change_the_ui() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::with_scale(560, 320, 2.0).expect("offscreen GPU");
    ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
    for font in FONTS {
        ui.load_font_data(font.to_vec());
    }
    let state = DemoState::default();
    let fixture = state.clone();
    ui.mount(move |ctx| build("dialog", &fixture, (ctx.width, ctx.height)))
        .settle();
    ui.click("#demo-action").settle();
    assert!(ui.find(".cn-dialog").is_some());
    let position = ui.centre(".cn-button--primary");
    let before = ui.snapshot().unwrap();
    let annotated = ui
        .snapshot_with_overlay(&cursor::annotation(position.0, position.1, true, Some(0.4)))
        .unwrap();
    assert!(before.diff_pixel_count(&annotated) > 100);
    assert!(
        !state.confirmed.get(),
        "annotation does not dispatch a click"
    );
    assert_eq!(ui.centre(".cn-button--primary"), position);
    ui.click(".cn-button--primary").settle();
    assert!(state.confirmed.get());
}

#[test]
fn hover_stays_visible_and_settles_with_and_without_overrides() {
    if !Headless::available() {
        return;
    }
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        for accent in [false, true] {
            let mut ui = Headless::new(560, 320).expect("offscreen GPU");
            ui.theme(theme_bundle(), scheme).css(DEMO_CSS);
            if accent {
                ui.css(include_str!("fixtures/violet.css"));
            }
            for font in FONTS {
                ui.load_font_data(font.to_vec());
            }
            let fixture = DemoState::default();
            ui.mount(move |ctx| build("button", &fixture, (ctx.width, ctx.height)))
                .settle();
            let node = ui.find("#demo-action").unwrap();
            let stable = ui.tree().stable_id(node).unwrap();
            let bounds = ui.tree().get_absolute_bounds(node).unwrap();
            let point = (
                (bounds.x + 6.0) as u32,
                (bounds.y + bounds.height * 0.5) as u32,
            );
            let pointer = ui.centre("#demo-action");
            ui.move_to(pointer.0, pointer.1);
            let mut settled = Vec::new();
            for frame in 0..40 {
                if frame % 2 == 0 {
                    ui.move_to(pointer.0, pointer.1);
                }
                ui.advance(16);
                assert_eq!(
                    ui.tree().stable_id(ui.find("#demo-action").unwrap()),
                    Some(stable)
                );
                assert!(
                    ui.is_hovered("#demo-action"),
                    "router lost hover on frame {frame}"
                );
                let image = ui.snapshot().unwrap();
                let pixel = image.get_pixel(point.0, point.1).unwrap();
                let background = image.get_pixel(0, 0).unwrap();
                let distance: u32 = pixel[..3]
                    .iter()
                    .zip(&background[..3])
                    .map(|(a, b)| a.abs_diff(*b) as u32)
                    .sum();
                assert!(distance > 60, "button vanished on frame {frame}: {pixel:?}");
                if frame >= 25 {
                    settled.push(pixel);
                }
            }
            let expected = if accent {
                [109u8, 40, 217]
            } else if scheme == ColorScheme::Light {
                [31, 82, 209]
            } else {
                [148, 184, 255]
            };
            assert!(
                settled.last().unwrap()[..3]
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "wrong hover target: {settled:?}"
            );
            assert!(
                settled.windows(2).all(|pair| pair[0][..3]
                    .iter()
                    .zip(&pair[1][..3])
                    .all(|(a, b)| a.abs_diff(*b) <= 1)),
                "hover restarted: {settled:?}"
            );
            assert!(
                ui.tree().css_transitions_empty(),
                "settled hover must go idle"
            );
            ui.mouse_down().advance(120).mouse_up();
            let mut after_click = Vec::new();
            for frame in 0..30 {
                ui.move_to(pointer.0, pointer.1).advance(60);
                let image = ui
                    .snapshot_with_overlay(&cursor::annotation(pointer.0, pointer.1, false, None))
                    .unwrap();
                assert!(ui.is_hovered("#demo-action"), "release lost hover");
                let pixel = image.get_pixel(point.0, point.1).unwrap();
                if frame > 10 {
                    after_click.push(pixel);
                }
            }
            assert!(
                after_click.iter().all(|pixel| pixel[..3]
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 1)),
                "hover after click cycles: {after_click:?}"
            );
            assert!(
                ui.tree().css_transitions_empty(),
                "post-click hover must settle"
            );
        }
    }
}

#[test]
fn chart_fits_the_container_width() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(300, 320).expect("offscreen GPU");
    ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
    for font in FONTS {
        ui.load_font_data(font.to_vec());
    }
    let fixture = DemoState::default();
    ui.mount(move |ctx| build("line-chart", &fixture, (ctx.width, ctx.height)))
        .settle();
    let chart = ui.find(".cn-chart").expect("chart");
    let bounds = ui.tree().get_absolute_bounds(chart).unwrap();
    assert!(bounds.x >= 0.0 && bounds.x + bounds.width <= 300.0);
}

#[test]
fn every_chart_example_fits_a_constrained_container() {
    if !Headless::available() {
        return;
    }
    for &slug in blinc_component_demos::SLUGS
        .iter()
        .filter(|slug| blinc_component_demos::is_chart(slug))
    {
        let mut ui = Headless::new(360, 320).expect("offscreen GPU");
        ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
        for font in FONTS {
            ui.load_font_data(font.to_vec());
        }
        let fixture = DemoState::default();
        ui.mount(move |ctx| build(slug, &fixture, (ctx.width, ctx.height)))
            .settle();
        let chart = ui.find(".cn-chart").expect(slug);
        let bounds = ui.tree().get_absolute_bounds(chart).unwrap();
        assert!(
            bounds.x >= 0.0 && bounds.x + bounds.width <= 360.5,
            "{slug} overflows its container: {bounds:?}"
        );
        assert!(bounds.height > 0.0, "{slug} has no plot");
        ui.snapshot().expect("chart pixels");
    }
}

#[test]
fn input_text_follows_edits_after_a_fresh_viewport_rebuild() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(560, 320).expect("offscreen GPU");
    ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
    let state = DemoState::default();
    let fixture = state.clone();
    ui.mount(move |ctx| build("input", &fixture, (ctx.width, ctx.height)))
        .settle();
    for count in 1..=3 {
        ui.rebuild_fresh().click(".cn-input");
        let node = ui.find(".cn-input").unwrap();
        let mut event = blinc_layout::event_handler::EventContext::new(
            blinc_core::events::event_types::TEXT_INPUT,
            node,
        )
        .with_key_char('A');
        event.stable_id = ui.tree().stable_id(node).unwrap();
        ui.tree().handler_registry().dispatch(&event);
        ui.advance(16);
        let expected = "A".repeat(count);
        assert_eq!(state.name.get(), expected);
        let tree = ui.tree();
        let mut nodes = vec![tree.root().unwrap()];
        let mut shown = Vec::new();
        while let Some(id) = nodes.pop() {
            if let Some(node) = tree.get_render_node(id) {
                if let blinc_layout::renderer::ElementType::Text(text) = &node.element_type {
                    shown.push(text.content.clone());
                }
            }
            nodes.extend(tree.layout_tree.children(id));
        }
        assert!(
            shown.contains(&expected),
            "field did not update after rebuild {count}: {shown:?}"
        );
    }
}

#[test]
fn action_demos_center_the_button_above_its_caption() {
    if !Headless::available() {
        return;
    }
    for slug in ["button", "dialog", "progress"] {
        let mut ui = Headless::new(560, 320).expect("offscreen GPU");
        ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
        let state = DemoState::default();
        ui.mount(move |ctx| build(slug, &state, (ctx.width, ctx.height)))
            .settle();
        let tree = ui.tree();
        let button = tree
            .get_absolute_bounds(ui.find("#demo-action").unwrap())
            .unwrap();
        let caption = tree
            .get_absolute_bounds(ui.find(".demo-caption").unwrap())
            .unwrap();
        assert!(
            (button.x + button.width / 2.0 - 280.0).abs() < 1.0,
            "{slug} button not centered: {button:?}"
        );
        assert!(
            (caption.x + caption.width / 2.0 - 280.0).abs() < 1.0,
            "{slug} caption not centered: {caption:?}"
        );
        if slug != "progress" {
            assert!(
                caption.y >= button.y + button.height + 12.0,
                "{slug} must stack its caption below the button"
            );
        }
    }
}
