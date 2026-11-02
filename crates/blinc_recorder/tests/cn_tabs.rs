#![cfg(feature = "headless")]
//! The pill behind the active tab slides to the next one and takes its width.

use blinc_core::reactive::signal;
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::text::text;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(400, 120).ok()?;
    let css = format!(
        "{}\n.cn-tabs-indicator {{ background: #ff0000; box-shadow: none; }}",
        blinc_cn::cn_styles::CN_STYLES
    );
    ui.theme(HybridTheme::bundle().with_css(&css), ColorScheme::Dark);
    Some(ui)
}

/// The tab triggers, in order.
fn triggers(ui: &Headless) -> Vec<LayoutNodeId> {
    let tree = ui.tree();
    let registry = tree.element_registry();
    let mut found = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if registry.has_class(node, "cn-tabs-trigger") {
            found.push(node);
        }
        let mut children = tree.layout_tree.children(node);
        children.reverse();
        stack.extend(children);
    }
    found
}

/// The left edge and width of the red pill along row `y`.
fn pill(ui: &mut Headless, y: u32) -> (u32, u32) {
    let frame = ui.snapshot().expect("snapshot");
    let red = |x: &u32| {
        let i = ((y * frame.width + x) * 4) as usize;
        frame.data[i] > 200 && frame.data[i + 1] < 60 && frame.data[i + 2] < 60
    };
    let left = (0..frame.width).find(red).expect("no pill on the row");
    let right = (0..frame.width).rev().find(red).unwrap();
    (left, right - left)
}

#[test]
fn the_pill_slides_to_the_next_tab() {
    let Some(mut ui) = session() else {
        return;
    };
    let selected = signal("a".to_string());
    ui.mount(move |_| {
        div().w(400.0).h(120.0).child(
            blinc_cn::tabs(selected)
                .tab("a", "A", || div().child(text("first")))
                .tab("b", "A much longer label", || div().child(text("second"))),
        )
    });
    ui.leave().settle();
    let next = ui.tree().get_absolute_bounds(triggers(&ui)[1]).unwrap();
    let y = (next.y + next.height / 2.0) as u32;
    let (from_x, from_w) = pill(&mut ui, y);

    ui.move_to(next.x + next.width / 2.0, next.y + next.height / 2.0)
        .mouse_down()
        .mouse_up();
    ui.advance(60);
    let (mid_x, mid_w) = pill(&mut ui, y);
    ui.settle();
    let (to_x, to_w) = pill(&mut ui, y);

    assert!(
        to_x > from_x + 10,
        "the pill did not move: {from_x} to {to_x}"
    );
    assert!(
        to_w > from_w + 40,
        "the pill did not widen: {from_w} to {to_w}"
    );
    assert!(
        mid_x > from_x + 2 && mid_x < to_x - 2,
        "the pill jumped: {mid_x} between {from_x} and {to_x}"
    );
    assert!(
        mid_w > from_w + 2 && mid_w < to_w - 2,
        "the pill's width jumped: {mid_w} between {from_w} and {to_w}"
    );
}
