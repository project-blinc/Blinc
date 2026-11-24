#![cfg(feature = "headless")]
//! Tabs made again by a region when a tab is added keep their padding as the selection leaves
//! them and while they are hovered.

use blinc_cn::prelude::*;
use blinc_core::reactive::{computed, signal};
use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_layout::renderer::ElementType;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

fn trigger_id(ui: &Headless, value: &str) -> String {
    let suffix = format!(":{value}");
    ui.tree()
        .element_registry()
        .all_ids()
        .into_iter()
        .find(|id| id.ends_with(&suffix))
        .expect("the trigger")
}

/// How far trigger `value`'s label sits from the trigger's left edge.
fn inset(ui: &Headless, value: &str) -> f32 {
    let tree = ui.tree();
    let trigger = ui.find(&format!("#{}", trigger_id(ui, value))).unwrap();
    let left = tree.get_absolute_bounds(trigger).unwrap().x;
    let mut stack = vec![trigger];
    while let Some(n) = stack.pop() {
        if let ElementType::Text(_) = &tree.get_render_node(n).unwrap().element_type {
            return tree.get_absolute_bounds(n).unwrap().x - left;
        }
        stack.extend(tree.layout().children(n));
    }
    panic!("no label")
}

#[test]
fn an_added_tab_keeps_its_padding_when_deselected_and_hovered() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(480, 160).unwrap();
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    let names = signal(vec!["one".to_string()]);
    let selected = signal("one".to_string());
    ui.mount(move |_| {
        div().w_full().h_full().for_each(
            computed(move |g| vec![g.get(names).unwrap_or_default()]),
            |names: &Vec<String>| names.join("/"),
            move |names: Vec<String>| {
                let mut strip = tabs(selected).reorderable(|_| {});
                for name in names {
                    strip = strip.tab(name.clone(), name.clone(), || div());
                }
                div().w_full().h_full().child(strip)
            },
        )
    });
    ui.settle();
    let padded = inset(&ui, "one");
    names.set(vec!["one".to_string(), "two".to_string()]);
    selected.set("two".to_string());
    ui.advance(300).settle();
    selected.set("one".to_string());
    ui.advance(300).settle();
    assert_eq!(
        inset(&ui, "two"),
        padded,
        "the added tab lost its padding when deselected"
    );

    let (x, y) = ui.centre(&format!("#{}", trigger_id(&ui, "two")));
    ui.move_to(x, y);
    ui.advance(300).settle();
    assert_eq!(
        inset(&ui, "two"),
        padded,
        "hovering the added tab lost its padding"
    );
}
