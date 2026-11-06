#![cfg(feature = "capture")]
use blinc_component_demos::{
    DEMO_CSS, FONTS, SLUGS,
    demos::{DemoState, build},
    theme_bundle,
};
use blinc_recorder::Headless;
use blinc_theme::ColorScheme;

#[test]
fn every_catalog_fixture_is_registered_once() {
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../web/components/catalog.json")).unwrap();
    let mut expected = Vec::new();
    for c in catalog["components"].as_array().unwrap() {
        expected.push(c["slug"].as_str().unwrap());
        if let Some(examples) = c["examples"].as_array() {
            expected.extend(examples.iter().map(|v| v["slug"].as_str().unwrap()));
        }
    }
    expected.sort_unstable();
    let mut actual = SLUGS.to_vec();
    actual.sort_unstable();
    assert_eq!(actual, expected);
    assert!(actual.windows(2).all(|pair| pair[0] != pair[1]));
}

#[test]
fn editor_pixels_follow_typing_after_fresh_rebuilds() {
    if !Headless::available() {
        return;
    }
    for (slug, selector) in [
        ("textarea", ".cn-textarea"),
        ("code-editor", ".blinc-code"),
        ("rich-text-editor", ".blinc-rich-text"),
    ] {
        let mut ui = Headless::new(560, 320).unwrap();
        ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
        for font in FONTS {
            ui.load_font_data(font.to_vec());
        }
        let state = DemoState::default();
        let fixture = state.clone();
        ui.mount(move |ctx| build(slug, &fixture, (ctx.width, ctx.height)))
            .settle();
        for pass in 0..2 {
            ui.rebuild_fresh().click(selector).advance(16);
            let bounds = ui
                .tree()
                .get_absolute_bounds(ui.find(selector).unwrap())
                .unwrap();
            let before = ui.snapshot().unwrap();
            ui.type_text("Visible text").advance(16);
            let after = ui.snapshot().unwrap();
            let mut changed = 0;
            for y in bounds.y.max(0.0) as u32..((bounds.y + bounds.height) as u32).min(320) {
                for x in bounds.x.max(0.0) as u32..((bounds.x + bounds.width) as u32).min(560) {
                    let a = before.get_pixel(x, y).unwrap();
                    let b = after.get_pixel(x, y).unwrap();
                    if a[..3]
                        .iter()
                        .zip(&b[..3])
                        .map(|(a, b)| a.abs_diff(*b) as u32)
                        .sum::<u32>()
                        > 50
                    {
                        changed += 1;
                    }
                }
            }
            assert!(
                changed > 100,
                "{slug}: typed text did not paint after rebuild {pass}: {changed} pixels"
            );
        }
    }
}

#[test]
fn virtual_list_replaces_its_window_and_reaches_both_ends() {
    if !Headless::available() {
        return;
    }
    use blinc_layout::{prelude::*, selector::ScrollRef};
    let reference = ScrollRef::new();
    let mounted = reference.clone();
    let mut ui = Headless::new(560, 320).unwrap();
    ui.theme(theme_bundle(), ColorScheme::Dark)
        .mount(move |_| {
            div().w(400.0).h(216.0).child(
                blinc_layout::widgets::virtual_list::virtual_list(10_000, |i| {
                    div()
                        .id(&format!("row-{i}"))
                        .class("test-row")
                        .h(if i % 2 == 0 { 40.0 } else { 52.0 })
                        .child(text(format!("Message {}", i + 1)))
                })
                .estimated_item_height(46.0)
                .window_size(24)
                .w_full()
                .h(216.0)
                .bind(&mounted)
                .into_div(),
            )
        })
        .settle();
    assert!(ui.find("#row-0").is_some());
    ui.move_to(100.0, 100.0)
        .scroll(0.0, -1600.0)
        .advance(160)
        .settle();
    assert!(ui.find("#row-0").is_none(), "old rows must unmount");
    let rows = ui.tree().element_registry().class_to_nodes_index();
    assert!(
        rows.get("test-row").unwrap().len() <= 40,
        "bounded mounted rows"
    );
    ui.rebuild_fresh().settle();
    assert!(
        ui.find("#row-0").is_none(),
        "fresh builds preserve the scroll position"
    );
    reference.scroll_to_bottom();
    ui.advance(160).settle();
    assert!(ui.find("#row-9999").is_some(), "last row is reachable");
    let last = ui
        .tree()
        .get_absolute_bounds(ui.find("#row-9999").unwrap())
        .unwrap();
    assert!(
        last.y >= -1.0 && last.y + last.height <= 217.0,
        "last row in viewport: {last:?}"
    );
    reference.scroll_to_top();
    ui.advance(160).settle();
    assert!(ui.find("#row-0").is_some(), "return to first row");
}

#[test]
fn aspect_ratio_css_controls_native_dimensions_and_card_contains_footer() {
    if !Headless::available() {
        return;
    }
    use blinc_layout::prelude::*;
    let mut ui = Headless::new(560, 400).unwrap();
    ui.theme(theme_bundle(), ColorScheme::Dark)
        .css(".cn-aspect-ratio { width:240px; aspect-ratio:4 / 3; }")
        .mount(|_| div().child(blinc_cn::aspect_ratio_16_9()))
        .settle();
    let b = ui
        .tree()
        .get_absolute_bounds(ui.find(".cn-aspect-ratio").unwrap())
        .unwrap();
    assert!(
        (b.width - 240.0).abs() < 1.0 && (b.height - 180.0).abs() < 1.0,
        "CSS ratio: {b:?}"
    );
    let state = DemoState::default();
    ui.theme(theme_bundle(), ColorScheme::Dark)
        .css(DEMO_CSS)
        .mount(move |ctx| build("card", &state, (ctx.width, ctx.height)))
        .settle();
    let card = ui
        .tree()
        .get_absolute_bounds(ui.find(".cn-card").unwrap())
        .unwrap();
    let footer = ui
        .tree()
        .get_absolute_bounds(ui.find(".cn-card-footer").unwrap())
        .unwrap();
    assert!(
        footer.y >= card.y && footer.y + footer.height <= card.y + card.height,
        "footer {footer:?} inside card {card:?}"
    );
}

#[test]
fn navigation_panel_is_anchored_and_reopens_after_escape() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(560, 320).unwrap();
    ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
    for font in FONTS {
        ui.load_font_data(font.to_vec());
    }
    let fixture = DemoState::default();
    ui.mount(move |ctx| build("navigation-menu", &fixture, (ctx.width, ctx.height)))
        .settle();
    let classes = ui.tree().element_registry().class_to_nodes_index();
    let mut triggers = classes["cn-nav-link"]
        .iter()
        .filter_map(|id| ui.tree().get_absolute_bounds(*id))
        .collect::<Vec<_>>();
    triggers.sort_by(|a, b| a.x.total_cmp(&b.x));
    let trigger = triggers.last().unwrap();
    let (x, y) = (
        trigger.x + trigger.width / 2.0,
        trigger.y + trigger.height / 2.0,
    );
    ui.move_to(x, y).advance(200).settle();
    let panel = ui
        .tree()
        .get_absolute_bounds(ui.find(".cn-nav-menu-content").expect("navigation opens"))
        .unwrap();
    assert!(
        panel.x >= trigger.x - 1.0 && panel.y >= trigger.y + trigger.height,
        "panel {panel:?} under trigger {trigger:?}"
    );
    ui.key(27).advance(400).settle();
    assert!(ui.find(".cn-nav-menu-content").is_none());
    ui.mouse_down().mouse_up().advance(200).settle();
    assert!(
        ui.find(".cn-nav-menu-content").is_some(),
        "click reopens without requiring a pointer exit"
    );
    ui.key(27).advance(400).settle();
}

#[test]
fn bottom_sheet_keeps_its_completion_action_inside_the_viewport() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(560, 480).unwrap();
    ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
    let fixture = DemoState::default();
    ui.mount(move |ctx| build("sheet", &fixture, (ctx.width, ctx.height)))
        .settle()
        .click("#demo-action")
        .advance(400)
        .settle();
    let sheet = ui
        .tree()
        .get_absolute_bounds(ui.find(".cn-sheet").unwrap())
        .unwrap();
    let footer = ui
        .tree()
        .get_absolute_bounds(ui.find(".cn-sheet-footer").unwrap())
        .unwrap();
    assert!(
        footer.y + footer.height <= 481.0 && footer.y >= sheet.y,
        "sheet {sheet:?}, footer {footer:?}"
    );
    let icon = ui
        .tree()
        .get_absolute_bounds(ui.find(".cn-icon").unwrap())
        .unwrap();
    let copy = ui
        .tree()
        .get_absolute_bounds(ui.find(".demo-share-permission-copy").unwrap())
        .unwrap();
    assert!(
        copy.x - icon.x - icon.width <= 18.0,
        "permission text follows its icon without distributed empty space: {icon:?}, {copy:?}"
    );
    let painted = ui.snapshot().unwrap();
    let top = painted.get_pixel(550, 100).unwrap();
    let middle = painted.get_pixel(550, 300).unwrap();
    assert!(
        top[..3]
            .iter()
            .zip(&middle[..3])
            .map(|(a, b)| a.abs_diff(*b) as u32)
            .sum::<u32>()
            < 12,
        "the settled sheet must paint at its bottom anchor, not retain a keyframe offset: {top:?} vs {middle:?}"
    );
    let done = ui
        .tree()
        .get_absolute_bounds(ui.find(".demo-share-done").unwrap())
        .unwrap();
    assert!(
        done.height > 20.0 && done.y + done.height <= 480.0,
        "completion action is reachable: {done:?}"
    );
    ui.click(".demo-share-done").advance(400).settle();
    assert!(ui.find(".cn-sheet").is_none());
}

#[test]
fn notch_menu_reveals_its_status_on_hover() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(680, 520).unwrap();
    ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
    let fixture = DemoState::default();
    ui.mount(move |ctx| build("notch", &fixture, (ctx.width, ctx.height)))
        .settle()
        .hover(".demo-notch-icon")
        .advance(600)
        .settle();
    assert!(
        ui.find(".demo-notch-status").is_some(),
        "interactive notched dropdown opens"
    );
}

#[test]
fn ratio_presets_and_notch_panel_fit_a_mobile_canvas() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(348, 400).unwrap();
    ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
    let fixture = DemoState::default();
    ui.mount(move |ctx| build("aspect-ratio", &fixture, (ctx.width, ctx.height)))
        .settle();
    let classes = ui.tree().element_registry().class_to_nodes_index();
    assert_eq!(classes["cn-aspect-ratio"].len(), 8);
    for id in &classes["cn-aspect-ratio"] {
        let b = ui.tree().get_absolute_bounds(*id).unwrap();
        assert!(
            b.x >= 23.0 && b.x + b.width <= 325.0 && b.y >= 23.0 && b.y + b.height <= 377.0,
            "ratio fits the padded viewport: {b:?}"
        );
    }
    let fixture = DemoState::default();
    ui.mount(move |_| build("notch", &fixture, (348.0, 400.0)))
        .settle()
        .hover(".demo-notch-icon")
        .advance(600)
        .settle();
    let panel = ui
        .tree()
        .get_absolute_bounds(ui.find(".demo-notch-status").unwrap())
        .unwrap();
    assert!(
        panel.x >= 0.0 && panel.x + panel.width <= 348.0,
        "notched dropdown stays within the viewport: {panel:?}"
    );
    let dock = ui
        .tree()
        .get_absolute_bounds(ui.find(".demo-notch-dock").unwrap())
        .unwrap();
    assert!(
        dock.y >= 0.0 && dock.y + dock.height <= 400.0,
        "dock remains visible while the examples scroll: {dock:?}"
    );
}

#[test]
fn overlay_close_buttons_center_their_icon_and_dismiss_once() {
    use blinc_cn as cn;
    use blinc_layout::prelude::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    if !Headless::available() {
        return;
    }
    for kind in ["sheet", "drawer"] {
        let mut ui = Headless::new(560, 480).unwrap();
        ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
        let closed = Arc::new(AtomicUsize::new(0));
        let calls = closed.clone();
        ui.mount(move |_| {
            let calls = calls.clone();
            div().child(cn::button("Open").on_click(move |_| {
                let calls = calls.clone();
                if kind == "sheet" {
                    cn::sheet()
                        .title("Details")
                        .on_close(move || {
                            calls.fetch_add(1, Ordering::SeqCst);
                        })
                        .show();
                } else {
                    cn::drawer()
                        .title("Files")
                        .on_close(move || {
                            calls.fetch_add(1, Ordering::SeqCst);
                        })
                        .show();
                }
            }))
        })
        .settle()
        .click(".cn-button")
        .advance(400)
        .settle();
        let selector = format!(".cn-{kind}-close");
        let button = ui.find(&selector).unwrap();
        let tree = ui.tree();
        assert!(tree.element_registry().has_class(button, "cn-button"));
        let b = tree.get_absolute_bounds(button).unwrap();
        let mut pending = tree.layout_tree.children(button);
        let mut icon = None;
        while let Some(node) = pending.pop() {
            let bounds = tree.get_absolute_bounds(node).unwrap();
            if (bounds.width - 16.0).abs() < 0.1 && (bounds.height - 16.0).abs() < 0.1 {
                icon = Some(bounds);
            }
            pending.extend(tree.layout_tree.children(node));
        }
        let icon = icon.expect("close SVG");
        assert!(
            (icon.x + icon.width / 2.0 - b.x - b.width / 2.0).abs() < 0.5,
            "{kind}: {b:?} {icon:?}"
        );
        assert!(
            (icon.y + icon.height / 2.0 - b.y - b.height / 2.0).abs() < 0.5,
            "{kind}: {b:?} {icon:?}"
        );
        ui.click(&selector).advance(400).settle();
        assert!(ui.find(&selector).is_none());
        assert_eq!(
            closed.load(Ordering::SeqCst),
            1,
            "{kind} dismissal callback"
        );
    }
}

#[test]
fn caller_disclosure_copy_matches_the_header_and_folds_completely() {
    if !Headless::available() {
        return;
    }
    for (slug, trigger, copy, body, css) in [
        (
            "collapsible",
            ".cn-collapsible-trigger",
            ".demo-collapsible-copy",
            ".cn-collapsible-content",
            include_str!("../../../web/components/recipes/collapsible.css"),
        ),
        (
            "accordion",
            ".cn-accordion-trigger",
            ".demo-accordion-copy",
            ".cn-accordion-content",
            include_str!("../../../web/components/recipes/accordion.css"),
        ),
    ] {
        let mut ui = Headless::new(560, 320).unwrap();
        ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
        let fixture = DemoState::default();
        ui.mount(move |ctx| build(slug, &fixture, (ctx.width, ctx.height)))
            .settle()
            .click(trigger)
            .advance(400)
            .settle();
        for recipe in ["", css] {
            ui.context().set_css("preview", recipe).unwrap();
            ui.settle();
            let tree = ui.tree();
            let first_child_x = |selector| {
                let node = ui.find(selector).unwrap();
                tree.get_absolute_bounds(tree.layout_tree.children(node)[0])
                    .unwrap()
                    .x
            };
            assert!((first_child_x(copy) - first_child_x(trigger)).abs() < 0.5);
        }
        ui.click(trigger).advance(400).settle();
        let body = ui.find(body).unwrap();
        assert!(ui.tree().get_absolute_bounds(body).unwrap().height < 0.5);
    }
}
