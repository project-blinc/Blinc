//! What a tabs strip looks like in each state, recorded as text. See
//! `common` for the walk; the record is `tabs_states.golden`.
//!
//! The content area swaps panels with a rebuild, which is not what is under
//! test, so the walk is recorded without the no-rebuild check and a second
//! test holds the strip to it for the pointer states.

mod common;

use blinc_cn::{TabsSize, TabsTransition, tab_item, tabs};
use blinc_layout::css_parser::{Stylesheet, set_active_stylesheet};
use blinc_layout::div::{ElementBuilder, div};
use blinc_layout::renderer::RenderTree;
use blinc_layout::text::text;
use common::*;
use std::fmt::Write as _;
use std::sync::Arc;

type Build = Box<dyn FnOnce(&blinc_core::reactive::State<String>) -> Box<dyn ElementBuilder>>;

fn panel(name: &'static str) -> impl Fn() -> blinc_layout::div::Div + Send + Sync + 'static {
    move || div().child(text(name))
}

/// The font weight of every text under `node`, in order.
fn text_weights(tree: &RenderTree, node: blinc_layout::LayoutNodeId) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(render) = tree.get_render_node(node) {
        if let blinc_layout::renderer::ElementType::Text(t) = &render.element_type {
            out.push(format!(
                "{:?}",
                render.props.font_weight.unwrap_or(t.weight)
            ));
        }
    }
    for child in tree.layout_tree.children(node) {
        out.extend(text_weights(tree, child));
    }
    out
}

/// The strip of triggers, one line per node under it.
fn describe_strip(tree: &RenderTree, widget: blinc_layout::LayoutNodeId, out: &mut String) {
    let strip = tree.layout_tree.children(widget)[0];
    describe_tree(tree, strip, out, 0);
    let _ = writeln!(out, "weights: {:?}", text_weights(tree, strip));
}

fn scenario(name: &str, in_place: bool, build: Build) -> String {
    set_active_stylesheet(Arc::new(Stylesheet::parse("").expect("css")));
    let selected = string_state("a");
    let widget = build(&selected);
    let mut h = Harness::new(widget, vec![selected.signal_id()], describe_strip);
    h.in_place = in_place;
    // The second trigger.
    let at = h.center(&[0, 1]);
    h.walk(at, (390.0, 195.0));
    format!("######## {name}\n{}", h.out)
}

fn two_tabs(state: &blinc_core::reactive::State<String>) -> Box<dyn ElementBuilder> {
    Box::new(
        tabs(state)
            .transition(TabsTransition::None)
            .tab("a", "Alpha", panel("alpha"))
            .tab("b", "Beta", panel("beta"))
            .tab("c", "Gamma", panel("gamma")),
    )
}

#[test]
fn a_tabs_strip_looks_the_same_in_every_state() {
    init();
    let got = guarded(|| {
        let mut out = String::new();
        out += &scenario("three tabs", false, Box::new(two_tabs));
        out += &scenario(
            "icons, a badge, a disabled tab, small",
            false,
            Box::new(|state| {
                Box::new(
                    tabs(state)
                        .transition(TabsTransition::None)
                        .size(TabsSize::Small)
                        .tab_item(
                            tab_item("a").label("Alpha").icon(blinc_icons::icons::CHECK),
                            panel("alpha"),
                        )
                        .tab_item(tab_item("b").label("Beta").badge("3"), panel("beta"))
                        .tab_item(tab_item("c").label("Gamma").disabled(), panel("gamma")),
                )
            }),
        );
        out += &scenario(
            "large",
            false,
            Box::new(|state| {
                Box::new(
                    tabs(state)
                        .transition(TabsTransition::None)
                        .size(TabsSize::Large)
                        .tab("a", "Alpha", panel("alpha"))
                        .tab("b", "Beta", panel("beta")),
                )
            }),
        );
        out
    });
    check_golden("tabs_states.golden", &got);
}

#[test]
fn the_pointer_alone_rebuilds_nothing() {
    init();
    guarded(|| {
        set_active_stylesheet(Arc::new(Stylesheet::parse("").expect("css")));
        let selected = string_state("a");
        let mut h = Harness::new(
            two_tabs(&selected),
            vec![selected.signal_id()],
            describe_strip,
        );
        h.in_place = true;
        let at = h.center(&[0, 1]);
        h.frame();
        h.move_to(at);
        h.press(at);
        h.move_to((390.0, 195.0));
    });
}
