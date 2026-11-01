//! Rows a `for_each` region builds are styled like any node: their own class
//! rules, rules that reach them through an ancestor's bound attribute, and
//! colour inherited from an ancestor, whether the row is built at mount, when
//! the list changes, or the ancestor's attribute flips afterwards.

use std::sync::Mutex;

use blinc_core::reactive::{Signal, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::binding::with_registry;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::text::text;

/// The pending-write queue and the reactive graph are process-wide.
static LOCK: Mutex<()> = Mutex::new(());

const CSS: &str = "
    .box { color: #0000ff; }
    .value { color: #00ff00; }
    .box:disabled .value { color: #ff0000; }
";

const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

struct Scene {
    tree: RenderTree,
    items: Signal<Vec<String>>,
    disabled: Signal<bool>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

fn scene(start_disabled: bool) -> Scene {
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    with_registry(|_| {});

    let items = signal(vec!["a".to_string()]);
    let disabled = signal(start_disabled);
    // A box whose rows are a classed text and a bare one that inherits.
    let ui = div().w(200.0).h(200.0).child(
        div()
            .class("box")
            .attr_when("disabled", "", disabled)
            .flex_col()
            .for_each(
                items,
                |item: &String| item.clone(),
                |item: String| {
                    div()
                        .flex_row()
                        .child(text(&item).class("value"))
                        .child(text(format!("{item}!")))
                },
            ),
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(CSS).expect("css"));
    tree.apply_all_stylesheet_styles();
    tree.compute_layout(200.0, 200.0);
    Scene {
        tree,
        items,
        disabled,
        _guard: guard,
    }
}

impl Scene {
    fn frame(&mut self) {
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(200.0, 200.0);
    }

    /// The classed text and the bare text of every row, in order.
    fn rows(&self) -> Vec<(LayoutNodeId, LayoutNodeId)> {
        let root = self.tree.root().unwrap();
        let bx = self.tree.layout_tree.children(root)[0];
        self.tree
            .layout_tree
            .children(bx)
            .into_iter()
            .map(|row| {
                let kids = self.tree.layout_tree.children(row);
                (kids[0], kids[1])
            })
            .collect()
    }

    fn colours(&self) -> Vec<([f32; 4], [f32; 4])> {
        self.rows()
            .into_iter()
            .map(|(value, bare)| {
                (
                    self.tree.resolved_text_color(value).expect("value colour"),
                    self.tree.resolved_text_color(bare).expect("bare colour"),
                )
            })
            .collect()
    }
}

#[test]
fn rows_built_at_mount_take_the_rules() {
    let s = scene(false);
    assert_eq!(s.colours(), vec![(GREEN, BLUE)]);
}

#[test]
fn rows_built_at_mount_under_a_disabled_ancestor_take_its_rule() {
    let s = scene(true);
    assert_eq!(s.colours(), vec![(RED, BLUE)]);
}

#[test]
fn rows_built_by_a_change_take_the_rules() {
    let mut s = scene(true);
    s.items.set(vec!["b".to_string(), "c".to_string()]);
    s.frame();
    assert_eq!(s.colours(), vec![(RED, BLUE), (RED, BLUE)]);
}

#[test]
fn rows_follow_the_ancestors_attribute_when_it_flips() {
    let mut s = scene(false);
    s.disabled.set(true);
    s.frame();
    assert_eq!(s.colours(), vec![(RED, BLUE)], "disabled");
    s.disabled.set(false);
    s.frame();
    assert_eq!(s.colours(), vec![(GREEN, BLUE)], "enabled again");
}

#[test]
fn a_row_built_after_the_flip_takes_the_current_state() {
    let mut s = scene(false);
    s.disabled.set(true);
    s.frame();
    s.items.set(vec!["d".to_string()]);
    s.frame();
    assert_eq!(s.colours(), vec![(RED, BLUE)]);
}
