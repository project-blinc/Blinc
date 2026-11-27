#![cfg(feature = "headless")]
//! The cn builders take a class and an id, which reach the element, and a labelled switch answers
//! a click on its label.

use blinc_cn::{button, checkbox, switch, toggle};
use blinc_core::reactive::signal;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

#[test]
fn builders_take_a_class_and_an_id() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(320, 240).unwrap();
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    let (checked, on, pressed) = (signal(false), signal(false), signal(false));
    ui.mount(move |_| {
        div()
            .w(320.0)
            .h(240.0)
            .flex_col()
            .gap(2.0)
            .child(button("Save").class("mine").id("a-button"))
            .child(checkbox(checked).class("mine").id("a-checkbox"))
            .child(switch(on).label("Wifi").class("mine").id("a-switch"))
            .child(toggle(pressed).label("Bold").class("mine").id("a-toggle"))
    });
    ui.leave().settle();

    for id in ["#a-button", "#a-checkbox", "#a-switch", "#a-toggle"] {
        let node = ui.find(id).unwrap_or_else(|| panic!("no {id}"));
        assert!(
            ui.tree().element_registry().has_class(node, "mine"),
            "{id} has no class"
        );
    }

    let track = ui.find("#a-switch").unwrap();
    assert!(
        ui.tree().element_registry().has_class(track, "cn-switch"),
        "the switch names its track"
    );

    ui.click(".cn-switch-label").settle();
    assert!(on.get(), "the label toggles the switch");
}

/// A session with the cn rules, showing an empty page.
fn session() -> Headless {
    let mut ui = Headless::new(800, 600).unwrap();
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui.mount(|_| div().w(800.0).h(600.0));
    ui.settle();
    ui
}

#[track_caller]
fn assert_named(ui: &Headless, id: &str) {
    let node = ui.find(id).unwrap_or_else(|| panic!("no {id}"));
    assert!(
        ui.tree().element_registry().has_class(node, "mine"),
        "{id} has no class"
    );
}

#[test]
fn every_element_builder_takes_a_class_and_an_id() {
    use blinc_cn::cn::*;
    use blinc_cn::components::collapsible::CollapsibleTrigger;
    use blinc_layout::widgets::code::code_editor_state;
    use blinc_layout::widgets::text_area::text_area_state;
    use blinc_layout::widgets::text_input::text_input_data;

    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    macro_rules! named {
        ($ids:ident, $($id:literal => $builder:expr),+ $(,)?) => {{
            let mut column = div().flex_col();
            $(
                column = column.child($builder.class("mine").id($id));
                $ids.push(concat!("#", $id));
            )+
            column
        }};
    }

    let (flag, text, number) = (signal(false), signal(String::new()), signal(0.5_f64));
    let (span, level) = (signal(0.2..=0.8_f64), signal(0.5_f32));
    let ids = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = ids.clone();
    let mut ui = session();
    ui.mount(move |_| {
        let mut ids: Vec<&'static str> = Vec::new();
        let (input_data, area, editor) =
            (text_input_data(), text_area_state(), code_editor_state("a"));
        let first = named!(ids,
            "n-aspect" => aspect_ratio(1.0),
            "n-breadcrumb" => breadcrumb(),
            "n-line" => line_chart(),
            "n-pie" => pie_chart(),
            "n-histogram" => histogram(vec![1.0, 2.0]),
            "n-comparison" => comparison_bar_chart(),
            "n-code" => code_editor(&editor),
            "n-collapsible" => collapsible(flag),
            "n-trigger" => CollapsibleTrigger::new("More", flag),
            "n-combobox" => combobox(text),
            "n-icon" => icon("M5 12h14"),
            "n-input" => input(&input_data),
            "n-otp" => input_otp(text, 4),
            "n-kbd" => kbd("K"),
            "n-label" => label("Name"),
            "n-gauge" => number_gauge(number),
            "n-range" => number_range_gauge(span),
            "n-number" => number_input(number),
        );
        let second = named!(ids,
            "n-progress" => progress(level),
            "n-scroll" => scroll_area(),
            "n-select" => select(text),
            "n-slider" => slider(level),
            "n-textarea" => textarea(&area),
            "n-group" => toggle_group(text),
            "n-header" => card_header(),
            "n-content" => card_content(),
            "n-footer" => card_footer(),
            "n-row" => table_row(),
            "n-radio" => radio_group(text).option("a", "A"),
            "n-alert" => alert("Saved"),
            "n-alert-box" => alert_box(),
            "n-badge" => badge("New"),
            "n-separator" => separator(),
            "n-skeleton" => skeleton(),
        );
        *seen.lock().unwrap() = ids;
        div()
            .w(800.0)
            .h(600.0)
            .flex_row()
            .child(first)
            .child(second)
            .child(
                resizable_group()
                    .panel(resizable_panel().class("mine-panel"))
                    .panel(resizable_panel()),
            )
    });
    ui.settle();

    let ids = ids.lock().unwrap().clone();
    assert!(ids.len() > 30);
    for id in ids {
        assert_named(&ui, id);
    }
    assert!(ui.find(".mine-panel").is_some(), "the panel has no class");
}

#[test]
fn dialogs_sheets_and_toasts_take_a_class_and_an_id() {
    use blinc_cn::cn::*;

    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = session();
    alert_dialog()
        .title("Delete?")
        .class("mine")
        .id("o-alert")
        .show();
    ui.settle();
    assert_named(&ui, "#o-alert");

    let mut ui = session();
    sheet().title("Filters").class("mine").id("o-sheet").show();
    ui.settle();
    assert_named(&ui, "#o-sheet");

    let mut ui = session();
    toast("Saved").class("mine").id("o-toast").show();
    ui.settle();
    assert_named(&ui, "#o-toast");
}

#[test]
fn a_named_context_menu_and_submenu_answer_a_press() {
    use blinc_cn::cn::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let pressed = Arc::new(AtomicUsize::new(0));
    let count = pressed.clone();
    let mut ui = session();
    context_menu()
        .at(40.0, 40.0)
        .class("mine")
        .id("o-menu")
        .submenu("More", move |sub| {
            let count = count.clone();
            sub.class("mine").id("o-sub").item("Run", move || {
                count.fetch_add(1, Ordering::SeqCst);
            })
        })
        .show();
    ui.settle();
    assert_named(&ui, "#o-menu");

    // The first item of the menu is the submenu's row.
    ui.click(".cn-context-menu-item").settle();
    assert_named(&ui, "#o-sub");

    let panel = ui.find("#o-sub").unwrap();
    let bounds = ui.tree().get_absolute_bounds(panel).unwrap();
    ui.move_to(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    )
    .mouse_down()
    .mouse_up()
    .settle();
    assert_eq!(pressed.load(Ordering::SeqCst), 1, "the submenu item ran");
}
