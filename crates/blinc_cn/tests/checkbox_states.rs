//! What a checkbox looks like in each state, recorded as text. See
//! `common` for the walk; the record is `checkbox_states.golden`.

mod common;

use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::{Stylesheet, set_active_stylesheet};
use blinc_layout::div::ElementBuilder;
use blinc_layout::renderer::RenderTree;
use common::*;
use std::fmt::Write as _;
use std::sync::Arc;

/// The box, whether a mark shows in it, and the label.
fn describe_checkbox(tree: &RenderTree, widget: LayoutNodeId, out: &mut String) {
    let kids = tree.layout_tree.children(widget);
    let _ = writeln!(out, "box: {}", node_line(tree, kids[0]));
    let _ = writeln!(
        out,
        "mark: {}",
        visible_svg(tree, kids[0]).unwrap_or_else(|| "none".into())
    );
    if let Some(label) = kids.get(1) {
        let _ = writeln!(out, "label: {}", node_line(tree, *label));
    }
}

fn scenario(
    name: &str,
    css: &str,
    in_place: bool,
    build: impl FnOnce(&blinc_core::reactive::State<bool>) -> Box<dyn ElementBuilder>,
) -> String {
    set_active_stylesheet(Arc::new(Stylesheet::parse(css).expect("css")));
    let checked = bool_state(false);
    let widget = build(&checked);
    let mut h = Harness::new(widget, vec![checked.signal_id()], describe_checkbox);
    h.in_place = in_place;
    let at = h.center(&[0]);
    h.walk(at, (300.0, 180.0));
    format!("######## {name}\n{}", h.out)
}

#[test]
fn a_checkbox_looks_the_same_in_every_state() {
    init();
    let got = guarded(|| {
        let mut out = String::new();
        out += &scenario("core with a label", "", true, |c| {
            Box::new(blinc_layout::widgets::checkbox(c).label("Accept"))
        });
        out += &scenario("core without a label", "", true, |c| {
            Box::new(blinc_layout::widgets::checkbox(c))
        });
        out += &scenario("core disabled", "", true, |c| {
            Box::new(
                blinc_layout::widgets::checkbox(c)
                    .label("Accept")
                    .disabled(true),
            )
        });
        out += &scenario(
            "core under hover and checked rules",
            "#cb:hover { background: #336699; border-color: #ff8800 } #cb:checked { background: #228844; accent-color: #ff0000 }",
            true,
            |c| Box::new(blinc_layout::widgets::checkbox(c).id("cb").label("Accept")),
        );
        out += &scenario("cn", "", true, |c| {
            Box::new(blinc_cn::checkbox(c).label("Accept"))
        });
        out
    });
    check_golden("checkbox_states.golden", &got);
}
