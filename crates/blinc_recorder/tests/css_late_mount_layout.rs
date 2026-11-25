#![cfg(feature = "headless")]
//! Layout rules reach a node that mounts after the first frame with a reactive attribute, and
//! follow an attribute or class that changes later.

use blinc_core::reactive::signal;
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const CSS: &str = "
.box { flex-direction: row; align-items: center; justify-content: flex-end; height: 40px; width: 100px; }
.box > .dot { width: 10px; height: 10px; }
.turns[data-state=\"turned\"] { flex-direction: column; align-items: flex-start; justify-content: flex-end; }
.self[data-state=\"on\"] { align-self: flex-end; }
";

fn one(ui: &Headless, class: &str) -> LayoutNodeId {
    let tree = ui.tree();
    let registry = tree.element_registry();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(n) = stack.pop() {
        if registry.has_class(n, class) {
            return n;
        }
        stack.extend(tree.layout_tree.children(n));
    }
    panic!("no .{class}");
}

/// Where `.dot` sits in the box `class`, from the box's corner.
fn dot_in(ui: &Headless, class: &str) -> (f32, f32) {
    let tree = ui.tree();
    let owner = one(ui, class);
    let b = tree.get_absolute_bounds(owner).unwrap();
    let dot = tree.layout_tree.children(owner)[0];
    let d = tree.get_absolute_bounds(dot).unwrap();
    (d.x - b.x, d.y - b.y)
}

#[test]
fn a_late_node_with_a_reactive_attribute_takes_the_layout_rules() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(300, 200).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    ui.css(CSS);
    let shown = signal(false);
    let turned = signal(false);
    ui.mount(move |_| {
        div()
            .w(300.0)
            .h(200.0)
            .flex_col()
            .child(div().show(&shown, move || {
                div()
                    .child(
                        div()
                            .class("box")
                            .class("late")
                            .class("turns")
                            .attr_when("data-state", "turned", turned)
                            .child(div().class("dot")),
                    )
                    .child(
                        div().class("box").class("outer").child(
                            div()
                                .class("dot")
                                .class("self")
                                .attr_when("data-state", "on", turned),
                        ),
                    )
            }))
    });
    ui.settle();
    shown.set(true);
    ui.settle();
    // At rest: in a row, centred across and against the end.
    assert_eq!(dot_in(&ui, "late"), (90.0, 15.0));
    assert_eq!(dot_in(&ui, "outer"), (90.0, 15.0));
    // The attribute turns the rules of the box and of the dot on.
    turned.set(true);
    ui.settle();
    assert_eq!(
        dot_in(&ui, "late"),
        (0.0, 30.0),
        "the column and its alignment"
    );
    assert_eq!(dot_in(&ui, "outer"), (90.0, 30.0), "align-self: flex-end");
}
