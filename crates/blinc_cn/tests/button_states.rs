//! What a button looks like in each state, recorded as text. See `common`
//! for the walk; the record is `button_states.golden`.

mod common;

use blinc_cn::{ButtonSize, ButtonVariant};
use blinc_layout::css_parser::{Stylesheet, set_active_stylesheet};
use blinc_layout::div::ElementBuilder;
use common::*;
use std::sync::Arc;

fn scenario(
    name: &str,
    css: &str,
    in_place: bool,
    build: impl FnOnce() -> Box<dyn ElementBuilder>,
) -> String {
    set_active_stylesheet(Arc::new(Stylesheet::parse(css).expect("css")));
    let mut h = Harness::new(build(), vec![], describe_tree_root);
    h.in_place = in_place;
    let at = h.center(&[]);
    h.walk(at, (380.0, 190.0));
    format!("######## {name}\n{}", h.out)
}

/// Every node, with what `node_line` leaves out: shadow, cursor and whether
/// the node lets the pointer through.
fn describe_tree_root(
    tree: &blinc_layout::renderer::RenderTree,
    node: blinc_layout::LayoutNodeId,
    out: &mut String,
) {
    describe_with_extras(tree, node, out, 0);
}

fn describe_with_extras(
    tree: &blinc_layout::renderer::RenderTree,
    node: blinc_layout::LayoutNodeId,
    out: &mut String,
    depth: usize,
) {
    use std::fmt::Write as _;
    let p = &tree.get_render_node(node).expect("render node").props;
    let _ = writeln!(
        out,
        "{}{} | shadow {:?} | cursor {:?} | passes pointer {}",
        "  ".repeat(depth),
        node_line(tree, node),
        p.shadow,
        p.cursor,
        p.pointer_events_none,
    );
    for child in tree.layout_tree.children(node) {
        describe_with_extras(tree, child, out, depth + 1);
    }
}

#[test]
fn a_button_looks_the_same_in_every_state() {
    init();
    let got = guarded(|| {
        let mut out = String::new();
        for (name, variant) in [
            ("primary", ButtonVariant::Primary),
            ("secondary", ButtonVariant::Secondary),
            ("destructive", ButtonVariant::Destructive),
            ("outline", ButtonVariant::Outline),
            ("ghost", ButtonVariant::Ghost),
            ("link", ButtonVariant::Link),
        ] {
            out += &scenario(name, "", true, || {
                Box::new(blinc_cn::button("Save").variant(variant))
            });
        }
        out += &scenario("small", "", true, || {
            Box::new(blinc_cn::button("Save").size(ButtonSize::Small))
        });
        out += &scenario("large with an icon after the label", "", true, || {
            Box::new(
                blinc_cn::button("Save")
                    .size(ButtonSize::Large)
                    .icon(blinc_icons::icons::CHECK)
                    .icon_position(blinc_cn::IconPosition::End),
            )
        });
        out += &scenario("icon only", "", true, || {
            Box::new(
                blinc_cn::button("")
                    .size(ButtonSize::Icon)
                    .icon(blinc_icons::icons::CHECK),
            )
        });
        out += &scenario("custom size", "", true, || {
            Box::new(blinc_cn::button("Save").size(ButtonSize::Custom(120.0, 48.0)))
        });
        out += &scenario("disabled", "", true, || {
            Box::new(blinc_cn::button("Save").disabled(true))
        });
        out += &scenario("text colour", "", true, || {
            Box::new(blinc_cn::button("Save").color(blinc_core::Color::rgb(1.0, 0.5, 0.0)))
        });
        out += &scenario(
            "state rules in a stylesheet",
            ".cn-button--primary:hover { background: #336699; color: #ffeedd } \
             .cn-button--primary:active { background: #224466; border-color: #ff8800 } \
             .cn-button--primary { border-width: 2px; border-color: #112233 }",
            true,
            || Box::new(blinc_cn::button("Save").icon(blinc_icons::icons::CHECK)),
        );
        out += &scenario(
            "state rules for a disabled button",
            ".cn-button--primary:disabled { background: #553322; color: #ddccbb }",
            true,
            || Box::new(blinc_cn::button("Save").disabled(true)),
        );
        out
    });
    check_golden("button_states.golden", &got);
}
