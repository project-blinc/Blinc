//! A button whose `disabled` follows a signal looks, in each state, exactly as
//! one built with that value would, and gets there without a rebuild.

mod common;

use blinc_cn::ButtonVariant;
use blinc_layout::css_parser::{Stylesheet, set_active_stylesheet};
use blinc_layout::div::ElementBuilder;
use common::*;
use std::sync::Arc;

/// A bound border that is not there is a transparent one of no width.
fn plain(look: String) -> String {
    look.replace("border 0.000,0.000,0.000,0.000 w0.0", "border -")
}

fn described(widget: Box<dyn ElementBuilder>, deps: Vec<blinc_core::reactive::SignalId>) -> String {
    let mut h = Harness::new(widget, deps, |tree, node, out| {
        describe_tree(tree, node, out, 0)
    });
    h.frame();
    h.record("look");
    h.out
}

#[test]
fn a_signal_that_disables_a_button_gives_it_the_look_of_a_disabled_one() {
    init();
    guarded(|| {
        set_active_stylesheet(Arc::new(Stylesheet::parse("").expect("css")));
        for variant in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Outline,
            ButtonVariant::Ghost,
        ] {
            let disabled = bool_state(false);
            let widget: Box<dyn ElementBuilder> = Box::new(
                blinc_cn::button("Save")
                    .variant(variant)
                    .disabled(&disabled),
            );
            let mut h = Harness::new(widget, vec![disabled.signal_id()], |tree, node, out| {
                describe_tree(tree, node, out, 0)
            });
            h.frame();

            disabled.set(true);
            h.frame();
            assert!(
                !blinc_layout::stateful::has_pending_subtree_rebuilds(),
                "disabling queued a rebuild"
            );
            h.record("look");
            let flipped_on = plain(h.out.clone());

            let built_disabled = described(
                Box::new(blinc_cn::button("Save").variant(variant).disabled(true)),
                vec![],
            );
            assert_eq!(
                flipped_on,
                plain(built_disabled),
                "{variant:?}: disabled by a signal does not look disabled"
            );

            disabled.set(false);
            let mut h2 = h;
            h2.out.clear();
            h2.frame();
            h2.record("look");
            let built_enabled =
                described(Box::new(blinc_cn::button("Save").variant(variant)), vec![]);
            assert_eq!(
                plain(h2.out.clone()),
                plain(built_enabled),
                "{variant:?}: enabled again by a signal does not look enabled"
            );
        }
    });
}
