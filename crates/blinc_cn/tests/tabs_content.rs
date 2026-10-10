//! What the content area of a tabs widget shows as the selection moves,
//! recorded as text. See `common` for the walk; the record is
//! `tabs_content.golden`.
//!
//! Each change is followed by enough frames for a transition to finish, so
//! the record is of the settled panel.

mod common;

use blinc_cn::{TabsTransition, tabs};
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::text::text;
use common::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn describe_content(tree: &RenderTree, widget: blinc_layout::LayoutNodeId, out: &mut String) {
    let content = tree.layout_tree.children(widget)[1];
    describe_tree(tree, content, out, 0);
}

/// Frames, a little apart, until a transition of 250ms has finished.
fn settle(h: &mut Harness, transition: TabsTransition) {
    let frames = if transition == TabsTransition::None {
        3
    } else {
        80
    };
    for _ in 0..frames {
        h.frame();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn scenario(name: &str, transition: TabsTransition, builds: &Arc<AtomicUsize>) -> String {
    let selected = string_state("a");
    let counted = Arc::clone(builds);
    let widget = tabs(&selected)
        .transition(transition)
        .tab("a", "Alpha", move || {
            counted.fetch_add(1, Ordering::SeqCst);
            div().child(text("alpha"))
        })
        .tab("b", "Beta", || div().child(text("beta")))
        .tab("c", "Gamma", || div().child(text("gamma")));
    let mut h = Harness::new(
        Box::new(widget),
        vec![selected.signal_id()],
        describe_content,
    );
    h.in_place = false;
    settle(&mut h, transition);
    h.record("initial");
    for next in ["b", "c", "a"] {
        selected.set(next.to_string());
        settle(&mut h, transition);
        h.record(&format!("selected {next}"));
    }
    format!("######## {name}\n{}", h.out)
}

#[test]
fn the_content_area_shows_the_selected_panel() {
    init();
    let got = guarded(|| {
        let builds = Arc::new(AtomicUsize::new(0));
        let mut out = String::new();
        out += &scenario("no transition", TabsTransition::None, &builds);
        out += &scenario("fade", TabsTransition::Fade, &builds);
        out
    });
    check_golden("tabs_content.golden", &got);
}
