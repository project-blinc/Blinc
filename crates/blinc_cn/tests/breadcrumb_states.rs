//! What a breadcrumb looks like in each state, recorded as text. See
//! `common` for the walk; the record is `breadcrumb_states.golden`.

mod common;

use blinc_cn::breadcrumb;
use blinc_layout::css_parser::{Stylesheet, set_active_stylesheet};
use blinc_layout::div::ElementBuilder;
use common::*;
use std::sync::Arc;

fn scenario(name: &str, build: impl FnOnce() -> Box<dyn ElementBuilder>) -> String {
    set_active_stylesheet(Arc::new(Stylesheet::parse("").expect("css")));
    let mut h = Harness::new(build(), vec![], |tree, node, out| {
        describe_tree(tree, node, out, 0)
    });
    h.in_place = true;
    // The first item: a link.
    let at = h.center(&[0]);
    h.walk(at, (380.0, 190.0));
    format!("######## {name}\n{}", h.out)
}

#[test]
fn a_breadcrumb_looks_the_same_in_every_state() {
    init();
    let got = guarded(|| {
        let mut out = String::new();
        out += &scenario("links and a current page", || {
            Box::new(
                breadcrumb()
                    .item("Home", || {})
                    .item("Library", || {})
                    .current("Data"),
            )
        });
        out += &scenario("with icons and a slash", || {
            Box::new(
                breadcrumb()
                    .item_with_icon("Home", blinc_icons::icons::CHECK, || {})
                    .item("Library", || {})
                    .current_with_icon("Data", blinc_icons::icons::PLUS)
                    .slash_separator(),
            )
        });
        out
    });
    check_golden("breadcrumb_states.golden", &got);
}
