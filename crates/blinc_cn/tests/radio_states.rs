//! What a radio group looks like in each state, recorded as text. See
//! `common` for the walk; the record is `radio_states.golden`.

mod common;

use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::{Stylesheet, set_active_stylesheet};
use blinc_layout::div::ElementBuilder;
use blinc_layout::renderer::RenderTree;
use common::*;
use std::fmt::Write as _;
use std::sync::Arc;

/// Each option: its row, its ring, the dot inside it if one shows, its label.
fn describe_radio(tree: &RenderTree, group: LayoutNodeId, out: &mut String) {
    for (i, row) in tree.layout_tree.children(group).into_iter().enumerate() {
        let parts = tree.layout_tree.children(row);
        let _ = writeln!(out, "option {i} row: {}", node_line(tree, row));
        let _ = writeln!(out, "option {i} ring: {}", node_line(tree, parts[0]));
        let dot = tree
            .layout_tree
            .children(parts[0])
            .into_iter()
            .find(|d| !tree.layout_tree.is_display_none(*d));
        let _ = writeln!(
            out,
            "option {i} dot: {}",
            dot.map(|d| node_line(tree, d))
                .unwrap_or_else(|| "none".into())
        );
        let _ = writeln!(out, "option {i} label: {}", node_line(tree, parts[1]));
    }
}

fn scenario(
    name: &str,
    css: &str,
    in_place: bool,
    build: impl FnOnce(&blinc_core::reactive::State<String>) -> Box<dyn ElementBuilder>,
) -> String {
    set_active_stylesheet(Arc::new(Stylesheet::parse(css).expect("css")));
    let selected = string_state("a");
    let widget = build(&selected);
    let mut h = Harness::new(widget, vec![selected.signal_id()], describe_radio);
    h.in_place = in_place;
    // Option b: the ring of the second row.
    let at = h.center(&[1, 0]);
    h.walk(at, (300.0, 180.0));
    format!("######## {name}\n{}", h.out)
}

#[test]
fn a_radio_group_looks_the_same_in_every_state() {
    init();
    let got = guarded(|| {
        let mut out = String::new();
        out += &scenario("core", "", true, |s| {
            Box::new(
                blinc_layout::widgets::radio_group(s)
                    .option("a", "Alpha")
                    .option("b", "Beta")
                    .option("c", "Gamma"),
            )
        });
        out += &scenario("core with a disabled option", "", true, |s| {
            Box::new(
                blinc_layout::widgets::radio_group(s)
                    .option("a", "Alpha")
                    .option_disabled("b", "Beta")
                    .option("c", "Gamma"),
            )
        });
        out += &scenario(
            "core under hover and checked rules",
            "#g-b { opacity: 0.8 } #g-b:hover { border-color: #ff0000; background: #223344 } #g-b:checked { background: #335577 }",
            true,
            |s| {
                Box::new(
                    blinc_layout::widgets::radio_group(s)
                        .id("g")
                        .option("a", "Alpha")
                        .option("b", "Beta")
                        .option("c", "Gamma"),
                )
            },
        );
        out += &scenario("cn", "", false, |s| {
            Box::new(
                blinc_cn::radio_group(s)
                    .option("a", "Alpha")
                    .option("b", "Beta")
                    .option("c", "Gamma"),
            )
        });
        out
    });
    check_golden("radio_states.golden", &got);
}
