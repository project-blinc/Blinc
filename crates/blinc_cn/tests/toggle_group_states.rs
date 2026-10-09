//! What a toggle group looks like in each state, recorded as text. See
//! `common` for the walk; the record is `toggle_group_states.golden`.

mod common;

use blinc_cn::{ToggleSize, ToggleVariant, toggle_group, toggle_item};
use blinc_layout::css_parser::{Stylesheet, set_active_stylesheet};
use blinc_layout::div::ElementBuilder;
use common::*;
use std::sync::Arc;

fn scenario(
    name: &str,
    in_place: bool,
    build: impl FnOnce(&blinc_core::reactive::State<String>) -> Box<dyn ElementBuilder>,
) -> String {
    set_active_stylesheet(Arc::new(Stylesheet::parse("").expect("css")));
    let selected = string_state("a");
    let widget = build(&selected);
    let mut h = Harness::new(widget, vec![selected.signal_id()], |tree, node, out| {
        describe_tree(tree, node, out, 0)
    });
    h.in_place = in_place;
    // The second item.
    let at = h.center(&[1]);
    h.walk(at, (380.0, 190.0));
    format!("######## {name}\n{}", h.out)
}

#[test]
fn a_toggle_group_looks_the_same_in_every_state() {
    init();
    let got = guarded(|| {
        let mut out = String::new();
        out += &scenario("default", true, |s| {
            Box::new(
                toggle_group(s)
                    .item(toggle_item("a").label("Left"))
                    .item(toggle_item("b").label("Centre"))
                    .item(toggle_item("c").label("Right")),
            )
        });
        out += &scenario("outline and small, with icons", true, |s| {
            Box::new(
                toggle_group(s)
                    .variant(ToggleVariant::Outline)
                    .size(ToggleSize::Small)
                    .item(toggle_item("a").icon(blinc_icons::icons::CHECK))
                    .item(toggle_item("b").icon(blinc_icons::icons::PLUS).label("Add"))
                    .item(toggle_item("c").icon(blinc_icons::icons::X)),
            )
        });
        out += &scenario("one item disabled", true, |s| {
            Box::new(
                toggle_group(s)
                    .item(toggle_item("a").label("Left"))
                    .item(toggle_item("b").label("Centre").disabled(true))
                    .item(toggle_item("c").label("Right")),
            )
        });
        out
    });
    check_golden("toggle_group_states.golden", &got);
}
