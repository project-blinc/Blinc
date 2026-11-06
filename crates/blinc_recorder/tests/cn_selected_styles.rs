#![cfg(feature = "headless")]

use blinc_core::{Brush, Color, reactive::signal};
use blinc_layout::{LayoutNodeId, div::div};
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

fn node(ui: &Headless, class: &str) -> LayoutNodeId {
    let tree = ui.tree();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if tree.element_registry().has_class(node, class) {
            return node;
        }
        stack.extend(tree.layout_tree.children(node));
    }
    panic!("missing {class}");
}

fn background(ui: &Headless, class: &str) -> Color {
    match ui
        .tree()
        .get_render_node(node(ui, class))
        .unwrap()
        .props
        .background
        .as_ref()
        .unwrap()
    {
        Brush::Solid(color) => *color,
        _ => panic!("solid {class}"),
    }
}

fn check_controls(scheme: ColorScheme) {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(480, 280).unwrap();
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        scheme,
    );
    let checked = signal(true);
    let disabled = signal(false);
    ui.mount(move |_| {
        div()
            .id("showcase")
            .w(480.0)
            .h(280.0)
            .flex_col()
            .gap_px(12.0)
            .p(16.0)
            .child(blinc_cn::checkbox(checked).disabled(disabled))
            .child(blinc_cn::switch(checked).disabled(disabled))
            .child(blinc_cn::toggle(checked).disabled(disabled).label("Bold"))
    });
    ui.leave().settle();
    let classes = ["cn-checkbox", "cn-switch", "cn-toggle"];
    let selected: Vec<_> = classes.iter().map(|class| background(&ui, class)).collect();
    let red = Color::from_hex(0xef4444);
    ui.context()
        .set_css(
            "app",
            r#"
        #showcase .cn-checkbox, #showcase .cn-switch, #showcase .cn-toggle {
            background: #ef4444;
            border-color: #ef4444;
            color: #ef4444;
        }
    "#,
        )
        .unwrap();
    ui.settle();
    for (class, expected) in classes.iter().zip(&selected) {
        assert_eq!(background(&ui, class), *expected, "selected {class}");
    }
    checked.set(false);
    ui.settle();
    for class in classes {
        assert_eq!(background(&ui, class), red, "unselected {class}");
    }
    checked.set(true);
    ui.settle();
    for (class, expected) in classes.iter().zip(&selected) {
        assert_eq!(background(&ui, class), *expected, "reselected {class}");
    }
    disabled.set(true);
    ui.settle();
    let disabled_bg = ThemeState::get().color(ColorToken::InputBgDisabled);
    for class in ["cn-checkbox", "cn-switch"] {
        assert_eq!(background(&ui, class), disabled_bg, "disabled {class}");
    }
    let frame = ui.snapshot().unwrap();
    assert!(frame.width > 0 && frame.height > 0);
}

#[test]
fn checked_controls_preserve_selection_and_disabled_colours_in_dark() {
    check_controls(ColorScheme::Dark);
}

#[test]
fn checked_controls_preserve_selection_and_disabled_colours_in_light() {
    check_controls(ColorScheme::Light);
}

#[test]
fn important_checked_borders_keep_their_transition() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(140, 48).unwrap();
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    let checked = signal(false);
    ui.mount(move |_| {
        div()
            .w(140.0)
            .h(48.0)
            .items_center()
            .justify_center()
            .child(blinc_cn::checkbox(checked).label("Accept"))
    });
    ui.leave().settle();
    let inspect = |ui: &Headless| {
        let props = &ui
            .tree()
            .get_render_node(node(ui, "cn-checkbox"))
            .unwrap()
            .props;
        props.border_color.unwrap()
    };
    ui.press(".cn-checkbox").advance(80);
    let before = inspect(&ui);
    ui.release();
    ui.advance(32);
    let middle = inspect(&ui);
    let target = ThemeState::get().color(ColorToken::Primary);
    assert_ne!(before, target);
    assert_ne!(middle, target, "border should still be transitioning");
    ui.settle();
    assert_eq!(inspect(&ui), target);
}

#[test]
fn active_tabs_restore_app_colours_when_the_selection_moves() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(400, 120).unwrap();
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui.context()
        .add_css("#showcase .cn-tabs-trigger { color: #ef4444; }");
    let selected = signal("a".to_string());
    ui.mount(move |_| {
        div().id("showcase").w(400.0).h(120.0).child(
            blinc_cn::tabs(selected)
                .tab("a", "Overview", || div())
                .tab("b", "Activity", || div()),
        )
    });
    ui.leave().settle();
    let original = node(&ui, "cn-tabs-trigger--active");
    let primary = ThemeState::get().color(ColorToken::TextPrimary);
    let colour = |ui: &Headless, id| {
        ui.tree()
            .get_render_node(id)
            .unwrap()
            .props
            .text_color
            .unwrap()
    };
    assert_eq!(
        colour(&ui, original),
        [primary.r, primary.g, primary.b, primary.a]
    );
    selected.set("b".to_string());
    ui.settle();
    let active = node(&ui, "cn-tabs-trigger--active");
    assert_ne!(original, active);
    let red = Color::from_hex(0xef4444);
    assert_eq!(colour(&ui, original), [red.r, red.g, red.b, red.a]);
    assert_eq!(
        colour(&ui, active),
        [primary.r, primary.g, primary.b, primary.a]
    );
}

#[test]
fn selecting_a_hovered_radio_continues_its_border_transition() {
    if !Headless::available() {
        return;
    }
    let mut ui = Headless::new(140, 96).unwrap();
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    let selected = signal("b".to_string());
    ui.mount(move |_| {
        div()
            .w(140.0)
            .h(96.0)
            .items_center()
            .justify_center()
            .child(
                blinc_cn::radio_group(selected)
                    .option("a", "First")
                    .option("b", "Second"),
            )
    });
    ui.leave().settle();
    let radio = ui.find(".cn-radio").unwrap();
    let colour = |ui: &Headless| {
        ui.tree()
            .get_render_node(radio)
            .unwrap()
            .props
            .border_color
            .unwrap()
    };
    ui.press(".cn-radio").advance(80);
    let before = colour(&ui);
    ui.release().advance(32);
    let middle = colour(&ui);
    assert_eq!(selected.get(), "a");
    assert!(
        middle.b >= before.b && middle.g >= before.g,
        "selection restarted the hover transition: {before:?} -> {middle:?}"
    );
    ui.settle();
    assert_eq!(colour(&ui), ThemeState::get().color(ColorToken::Primary));
}
